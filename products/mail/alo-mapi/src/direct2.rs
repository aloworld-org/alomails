//! LZ77 + DIRECT2 decompression ([MS-OXCRPC] §3.1.4.1.1.2).
//!
//! A compressed segment is a stream of literal bytes and back-references, told
//! apart by bitmasks inserted periodically:
//!
//! * A **4-byte bitmask**, consumed **least-significant bit first**. A `0` bit
//!   means the next input byte is an output byte; a `1` bit means metadata
//!   follows. When all 32 bits are used, the next four bytes are a new bitmask.
//! * **Metadata** is a little-endian `u16`: the low 3 bits are the length, the
//!   high 13 bits are the offset.
//!
//! ## The offset, which the specification states the long way round
//!
//! §3.1.4.1.1.2.2.2 says the 13 high bits are "a first complement of the
//! offset, represented as a negative signed value in 2's complement", and gives
//! one worked example: metadata `0x0018` yields offset bits `3`, and "the
//! offset is -4, computed by inverting the offset bits, treating the result as
//! a 2's complement, and converting it to an integer".
//!
//! Following that arithmetic through: inverting 3 across 13 bits gives 8188;
//! read as a signed 13-bit value that is −4; so the distance is 4. In general
//! the inversion gives `8191 − bits`, which as a signed 13-bit value is
//! `−1 − bits`, so the backward distance is simply **`bits + 1`**.
//!
//! It is written that way here — with the specification's own example pinned as
//! a test — because the arithmetic is the sort that looks right in three
//! different wrong ways, and every one of them decompresses to plausible
//! rubbish rather than to an error.
//!
//! ## The length, which carries state across the whole stream
//!
//! Lengths 3–9 live in the 3 bits (`bits + 3`). A value of `0b111` means "10 or
//! more", and the remainder comes from a **nibble shared between two
//! metadata instances**: the first long match reads a new byte and takes its
//! low nibble, remembering the high nibble for the *next* long match, which
//! reads no byte at all. That state persists for the whole stream, so a decoder
//! that treats each match independently drifts by one byte and never recovers.

use crate::rpc::RpcError;

/// The minimum match length; every encoded length is relative to it.
const MIN_MATCH: usize = 3;

/// Reads bytes and bits from the compressed stream.
struct Reader<'a> {
    input: &'a [u8],
    at: usize,
    /// The current bitmask, and how many of its bits remain.
    mask: u32,
    bits_left: u8,
    /// The high nibble left over from the last extended length, if any.
    spare_nibble: Option<u8>,
}

impl<'a> Reader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            at: 0,
            mask: 0,
            bits_left: 0,
            spare_nibble: None,
        }
    }

    fn byte(&mut self) -> Result<u8, RpcError> {
        let byte = *self
            .input
            .get(self.at)
            .ok_or(RpcError::Truncated { part: "payload" })?;
        self.at += 1;
        Ok(byte)
    }

    fn u16_le(&mut self) -> Result<u16, RpcError> {
        let low = self.byte()?;
        let high = self.byte()?;
        Ok(u16::from_le_bytes([low, high]))
    }

    /// The next flag bit, reading a fresh bitmask when the current one is spent.
    fn flag(&mut self) -> Result<bool, RpcError> {
        if self.bits_left == 0 {
            let mut raw = [0u8; 4];
            for slot in &mut raw {
                *slot = self.byte()?;
            }
            self.mask = u32::from_le_bytes(raw);
            self.bits_left = 32;
        }
        let bit = self.mask & 1 == 1;
        self.mask >>= 1;
        self.bits_left -= 1;
        Ok(bit)
    }

    /// Whether any input remains.
    fn spent(&self) -> bool {
        self.at >= self.input.len()
    }

    /// The next extended-length nibble.
    ///
    /// Every *other* long match consumes a byte: the first takes the low
    /// nibble and banks the high one, the next spends what was banked. That
    /// alternation is stream state, not per-match state.
    fn length_nibble(&mut self) -> Result<u8, RpcError> {
        match self.spare_nibble.take() {
            Some(nibble) => Ok(nibble),
            None => {
                let byte = self.byte()?;
                self.spare_nibble = Some(byte >> 4);
                Ok(byte & 0x0F)
            }
        }
    }
}

/// Decodes a match length once the in-line 3 bits said "10 or more".
fn extended_length(reader: &mut Reader<'_>) -> Result<usize, RpcError> {
    let nibble = reader.length_nibble()?;
    if nibble != 0x0F {
        // b'111' means ten, plus whatever the nibble adds.
        return Ok(10 + usize::from(nibble));
    }
    let extra = reader.byte()?;
    if extra != 0xFF {
        // Ten, plus a full nibble of fifteen, plus this byte.
        return Ok(25 + usize::from(extra));
    }
    // From 280 upward the final two bytes carry the whole length and are
    // **not** added to what came before — the one place in this encoding where
    // the parts do not accumulate.
    let total = reader.u16_le()?;
    Ok(usize::from(total) + MIN_MATCH)
}

/// Decompresses one DIRECT2 payload, expecting exactly `expected` bytes out.
///
/// `expected` is the segment's `SizeActual`. It is a bound, not a hint: the
/// stream's own end marker is a bit in a bitmask, which a truncated or hostile
/// payload can simply omit, so the output length is what actually stops us.
///
/// # Errors
/// [`RpcError::Truncated`] if the input ends mid-element, and
/// [`RpcError::SizeMismatch`] if the stream does not produce exactly the length
/// its header promised — a back-reference pointing before the start of the
/// output is reported the same way, since both mean the payload is not what it
/// claims to be.
pub fn decompress(input: &[u8], expected: usize) -> Result<Vec<u8>, RpcError> {
    let mut reader = Reader::new(input);
    let mut out: Vec<u8> = Vec::with_capacity(expected);

    while out.len() < expected {
        // A `1` bit with nothing left to read is the end-of-stream marker; a
        // `1` bit with input remaining introduces a match.
        let is_match = match reader.flag() {
            Ok(bit) => bit,
            // Running out of bitmask before the expected length is reached
            // means the payload is short of what its header promised.
            Err(_) => break,
        };

        if !is_match {
            let byte = reader.byte()?;
            out.push(byte);
            continue;
        }

        if reader.spent() {
            // The end marker: a set bit following the last element.
            break;
        }

        let metadata = reader.u16_le()?;
        // Low three bits: the length. High thirteen: the offset, one less than
        // the backward distance (see the module note).
        let length_bits = usize::from(metadata & 0x0007);
        let distance = usize::from(metadata >> 3) + 1;
        let length = if length_bits == 7 {
            extended_length(&mut reader)?
        } else {
            length_bits + MIN_MATCH
        };

        // A back-reference before the start of the output is not a decoding
        // subtlety, it is a malformed stream — and reading it would be reading
        // memory the payload never wrote.
        if distance > out.len() {
            return Err(RpcError::SizeMismatch { size: 0, actual: 0 });
        }
        // Overlapping matches are legal and load-bearing: a run is encoded as a
        // distance of one and a long length, so this copies byte by byte and
        // reads what it has just written.
        let start = out.len() - distance;
        for index in 0..length {
            if out.len() >= expected {
                break;
            }
            let byte = out[start + index];
            out.push(byte);
        }
    }

    if out.len() != expected {
        return Err(RpcError::SizeMismatch { size: 0, actual: 0 });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Builds a stream from a list of elements, laying out bitmasks the way an
    /// encoder does: bits accumulate least-significant first, and a fresh
    /// 4-byte mask is emitted every 32 elements.
    enum Element {
        Literal(u8),
        Match { distance: usize, length: usize },
        End,
    }

    fn encode(elements: &[Element]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut mask: u32 = 0;
        let mut bits = 0u8;
        let mut pending: Vec<u8> = Vec::new();
        let mut spare: Option<usize> = None;

        for element in elements {
            match element {
                Element::Literal(byte) => pending.push(*byte),
                Element::Match { distance, length } => {
                    mask |= 1 << bits;
                    let offset_bits = u16::try_from(distance - 1).unwrap();
                    if *length <= 9 {
                        let low = u16::try_from(length - MIN_MATCH).unwrap();
                        pending.extend_from_slice(&((offset_bits << 3) | low).to_le_bytes());
                    } else {
                        pending.extend_from_slice(&((offset_bits << 3) | 7).to_le_bytes());
                        let nibble = u8::try_from(length - 10).unwrap();
                        assert!(nibble < 0x0F, "test encoder covers lengths 10..24");
                        match spare.take() {
                            Some(at) => {
                                let byte: &mut u8 = &mut pending[at];
                                *byte |= nibble << 4;
                            }
                            None => {
                                spare = Some(pending.len());
                                pending.push(nibble);
                            }
                        }
                    }
                }
                Element::End => mask |= 1 << bits,
            }
            bits += 1;
            if bits == 32 {
                out.extend_from_slice(&mask.to_le_bytes());
                out.append(&mut pending);
                mask = 0;
                bits = 0;
            }
        }
        if bits > 0 {
            out.extend_from_slice(&mask.to_le_bytes());
            out.append(&mut pending);
        }
        out
    }

    /// The specification's own worked example, pinned: metadata `0x0018` is an
    /// offset of 4 back and a length of 3. If the offset arithmetic is wrong in
    /// any of the plausible ways, this is what catches it.
    #[test]
    fn the_specifications_worked_example_decodes_as_it_says() {
        let metadata: u16 = 0x0018;
        assert_eq!(metadata & 0x0007, 0, "length bits");
        assert_eq!(metadata >> 3, 3, "offset bits");
        // Inverting 3 across 13 bits, read as signed, is -4 — a distance of 4.
        let inverted = (!(metadata >> 3)) & 0x1FFF;
        let signed = i32::from(inverted) - 0x2000;
        assert_eq!(signed, -4);
        assert_eq!(usize::from(metadata >> 3) + 1, 4, "the shorthand agrees");

        // And end to end: "abcd" then a match of 3 bytes from 4 back.
        let stream = encode(&[
            Element::Literal(b'a'),
            Element::Literal(b'b'),
            Element::Literal(b'c'),
            Element::Literal(b'd'),
            Element::Match {
                distance: 4,
                length: 3,
            },
            Element::End,
        ]);
        assert_eq!(decompress(&stream, 7).unwrap(), b"abcdabc");
    }

    /// The specification's own illustration: "ABCABCDEF".
    #[test]
    fn the_specifications_abcabcdef_example_round_trips() {
        let stream = encode(&[
            Element::Literal(b'A'),
            Element::Literal(b'B'),
            Element::Literal(b'C'),
            Element::Match {
                distance: 3,
                length: 3,
            },
            Element::Literal(b'D'),
            Element::Literal(b'E'),
            Element::Literal(b'F'),
            Element::End,
        ]);
        assert_eq!(decompress(&stream, 9).unwrap(), b"ABCABCDEF");
    }

    #[test]
    fn a_stream_of_only_literals_is_returned_unchanged() {
        let elements: Vec<Element> = b"plain bytes, no matches at all"
            .iter()
            .map(|b| Element::Literal(*b))
            .chain(std::iter::once(Element::End))
            .collect();
        let stream = encode(&elements);
        assert_eq!(
            decompress(&stream, 30).unwrap(),
            b"plain bytes, no matches at all"
        );
    }

    /// A run is encoded as a distance of one and a long length, so the copy
    /// must read bytes it is itself writing. A decoder that copies the source
    /// range up front produces the wrong answer here and nowhere else.
    #[test]
    fn an_overlapping_match_repeats_what_it_is_writing() {
        let stream = encode(&[
            Element::Literal(b'x'),
            Element::Match {
                distance: 1,
                length: 6,
            },
            Element::End,
        ]);
        assert_eq!(decompress(&stream, 7).unwrap(), b"xxxxxxx");
    }

    /// The shared nibble alternates across the whole stream: the first long
    /// match banks a nibble the second spends. Treating each match on its own
    /// drifts by a byte and never recovers, which this catches.
    #[test]
    fn two_long_matches_share_one_extended_length_byte() {
        let stream = encode(&[
            Element::Literal(b'a'),
            Element::Literal(b'b'),
            Element::Literal(b'c'),
            Element::Literal(b'd'),
            // 12 bytes from 4 back, twice — the second reads no new byte.
            Element::Match {
                distance: 4,
                length: 12,
            },
            Element::Match {
                distance: 4,
                length: 12,
            },
            Element::End,
        ]);
        let out = decompress(&stream, 28).unwrap();
        assert_eq!(out.len(), 28);
        assert_eq!(&out[0..4], b"abcd");
        // Each match repeats the four-byte window three times.
        assert_eq!(&out[4..16], b"abcdabcdabcd");
        assert_eq!(&out[16..28], b"abcdabcdabcd");
    }

    /// A bitmask covers 32 elements; the 33rd needs a fresh one. An encoder
    /// that forgets is off by four bytes from there on.
    #[test]
    fn a_new_bitmask_is_read_every_thirty_two_elements() {
        let elements: Vec<Element> = (0..70u8)
            .map(Element::Literal)
            .chain(std::iter::once(Element::End))
            .collect();
        let stream = encode(&elements);
        let out = decompress(&stream, 70).unwrap();
        assert_eq!(out, (0..70u8).collect::<Vec<u8>>());
    }

    /// A back-reference reaching before the start of the output is a malformed
    /// stream, not a decoding subtlety — following it would read bytes the
    /// payload never wrote.
    #[test]
    fn a_match_pointing_before_the_start_is_refused() {
        let stream = encode(&[
            Element::Literal(b'a'),
            Element::Match {
                distance: 64,
                length: 3,
            },
            Element::End,
        ]);
        assert!(decompress(&stream, 4).is_err());
    }

    /// Every truncation is an error rather than a short, plausible output.
    #[test]
    fn a_truncated_stream_never_yields_a_partial_answer() {
        let stream = encode(&[
            Element::Literal(b'a'),
            Element::Literal(b'b'),
            Element::Literal(b'c'),
            Element::Literal(b'd'),
            Element::Match {
                distance: 4,
                length: 3,
            },
            Element::End,
        ]);
        for cut in 0..stream.len() {
            assert!(
                decompress(&stream[..cut], 7).is_err(),
                "accepted a stream cut at {cut}"
            );
        }
    }

    /// A stream that cannot produce the length its header promised is refused:
    /// returning whatever it managed would hand the ROP layer a truncated
    /// request to act on, which is the dangerous direction.
    ///
    /// The other direction — a stream carrying more than the header claimed —
    /// is **tolerated**, and that asymmetry is deliberate. We stop with exactly
    /// the promised bytes in hand, so there is nothing unsafe about ignoring
    /// what follows; refusing it would be strictness bought at the price of
    /// rejecting a real client over padding. Strict in what we send, tolerant
    /// in what we accept, within safety.
    #[test]
    fn a_stream_that_falls_short_of_its_header_is_refused() {
        let stream = encode(&[Element::Literal(b'a'), Element::Literal(b'b'), Element::End]);
        assert_eq!(decompress(&stream, 2).unwrap(), b"ab");
        assert!(decompress(&stream, 3).is_err(), "accepted a short stream");
        // Tolerated, and exactly the promised length is returned.
        assert_eq!(decompress(&stream, 1).unwrap(), b"a");
    }
}
