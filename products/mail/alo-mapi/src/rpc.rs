//! The extended buffer that carries ROP payloads ([MS-OXCRPC] §2.2.2.1).
//!
//! Every `Execute` request wraps its ROP payload in one or more `RPC_HEADER_EXT`
//! segments. The header is eight bytes:
//!
//! | Field | Size | Meaning |
//! |---|---|---|
//! | `Version` | 2 | MUST be `0x0000` |
//! | `Flags` | 2 | `Compressed` `0x0001`, `XorMagic` `0x0002`, `Last` `0x0004` |
//! | `Size` | 2 | payload length after this header |
//! | `SizeActual` | 2 | payload length once uncompressed |
//!
//! Segments chain: each is read, its payload decoded, and the next begins
//! immediately after — until one carries `Last`. The decoded payloads are
//! concatenated into the single ROP buffer the layer above sees.
//!
//! **`XorMagic` is not encryption and must never be mistaken for it.** The
//! specification is explicit that it exists "to obscure any easily readable
//! messaging data" and "is not intended as a security feature": every byte is
//! simply XORed with `0xA5`. Confidentiality on this protocol comes from TLS
//! and from nowhere else.
//!
//! **`Compressed` payloads are LZ77 + DIRECT2** ([`crate::direct2`]). A segment
//! may be both compressed and obfuscated, and the order is not interchangeable:
//! the client obfuscates the *compressed* bytes, so the XOR is undone first and
//! the result is what gets decompressed.

/// The fixed size of an `RPC_HEADER_EXT`.
pub const HEADER_LEN: usize = 8;

/// `Compressed` — the payload is Direct2-compressed.
pub const FLAG_COMPRESSED: u16 = 0x0001;
/// `XorMagic` — the payload is obfuscated by XOR with [`XOR_MAGIC`].
pub const FLAG_XOR_MAGIC: u16 = 0x0002;
/// `Last` — no further segment follows this one.
pub const FLAG_LAST: u16 = 0x0004;

/// The byte every obfuscated payload is XORed with ([MS-OXCRPC] §3.1.4.1.1.3).
pub const XOR_MAGIC: u8 = 0xA5;

/// The most we will assemble from one request's segments.
///
/// Each segment declares its own length, so without a ceiling a chain of
/// segments is an instruction to allocate without bound.
pub const MAX_ROP_BUFFER: usize = 8 * 1024 * 1024;

/// Why an extended buffer could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RpcError {
    /// The bytes ran out mid-header or mid-payload.
    #[error("extended buffer truncated in {part}")]
    Truncated {
        /// Which part ran out.
        part: &'static str,
    },
    /// `Version` was not `0x0000`.
    #[error("unsupported RPC_HEADER_EXT version {found:#06x}")]
    Version {
        /// What the client sent.
        found: u16,
    },
    /// A segment's declared lengths contradict each other, or a compressed
    /// payload did not expand to the length it promised.
    #[error("segment declares size {size} but actual {actual}")]
    SizeMismatch {
        /// The declared payload length.
        size: u16,
        /// The declared uncompressed length.
        actual: u16,
    },
    /// The chain never ended with a `Last` segment.
    #[error("extended buffer never terminated")]
    Unterminated,
    /// The assembled buffer is larger than we accept.
    #[error("ROP buffer exceeds {MAX_ROP_BUFFER} bytes")]
    TooLarge,
}

/// One parsed `RPC_HEADER_EXT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RpcHeaderExt {
    /// The flags governing how the payload is encoded.
    pub flags: u16,
    /// The payload length following this header.
    pub size: u16,
    /// The payload length once uncompressed.
    pub size_actual: u16,
}

impl RpcHeaderExt {
    /// Whether this is the final segment.
    #[must_use]
    pub const fn is_last(self) -> bool {
        self.flags & FLAG_LAST != 0
    }

    /// Whether the payload is obfuscated.
    #[must_use]
    pub const fn is_obfuscated(self) -> bool {
        self.flags & FLAG_XOR_MAGIC != 0
    }

    /// Whether the payload is compressed.
    #[must_use]
    pub const fn is_compressed(self) -> bool {
        self.flags & FLAG_COMPRESSED != 0
    }

    /// Reads a header from the front of `input`.
    ///
    /// # Errors
    /// [`RpcError::Truncated`] if fewer than eight bytes remain, or
    /// [`RpcError::Version`] if the version is not the one the specification
    /// fixes.
    pub fn parse(input: &[u8]) -> Result<Self, RpcError> {
        let head = input
            .get(..HEADER_LEN)
            .ok_or(RpcError::Truncated { part: "header" })?;
        let field = |at: usize| -> u16 {
            // Each read is exactly two bytes inside an eight-byte slice we
            // already hold, so the conversion cannot fail.
            u16::from_le_bytes([head[at], head[at + 1]])
        };
        let version = field(0);
        if version != 0x0000 {
            return Err(RpcError::Version { found: version });
        }
        Ok(Self {
            flags: field(2),
            size: field(4),
            size_actual: field(6),
        })
    }

    /// The eight bytes of this header.
    #[must_use]
    pub fn to_bytes(self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[0..2].copy_from_slice(&0u16.to_le_bytes()); // Version MUST be 0.
        out[2..4].copy_from_slice(&self.flags.to_le_bytes());
        out[4..6].copy_from_slice(&self.size.to_le_bytes());
        out[6..8].copy_from_slice(&self.size_actual.to_le_bytes());
        out
    }
}

/// Applies the obfuscation, which is its own inverse.
///
/// One function for both directions on purpose: it is an XOR, so encoding and
/// decoding are the same operation, and two functions would only invite them to
/// drift apart.
pub fn deobfuscate(payload: &mut [u8]) {
    for byte in payload.iter_mut() {
        *byte ^= XOR_MAGIC;
    }
}

/// Reads a chained extended buffer and returns the assembled ROP payload.
///
/// # Errors
/// [`RpcError`] if a segment is truncated, declares an impossible size, fails
/// to decompress to its promised length, or the chain never carries `Last`.
pub fn read_extended_buffer(mut input: &[u8]) -> Result<Vec<u8>, RpcError> {
    let mut out: Vec<u8> = Vec::new();
    loop {
        let header = RpcHeaderExt::parse(input)?;
        // An **uncompressed** segment whose two lengths disagree contradicts the
        // specification outright, and is refused rather than reconciled by
        // picking one.
        //
        // A compressed one is checked by decompressing it, not by comparing its
        // header to itself. The specification does say `Size` MUST be the
        // smaller, but enforcing that here would reject a client whose
        // compressor expanded a tiny payload — and gain nothing, because
        // `SizeActual` is a `u16` and the ceiling below already bounds what we
        // will allocate. The decompressor insists on producing exactly the
        // promised length, which is the check that actually matters.
        if !header.is_compressed() && header.size != header.size_actual {
            return Err(RpcError::SizeMismatch {
                size: header.size,
                actual: header.size_actual,
            });
        }

        let body_start = HEADER_LEN;
        let body_end = body_start
            .checked_add(usize::from(header.size))
            .ok_or(RpcError::Truncated { part: "payload" })?;
        let segment = input
            .get(body_start..body_end)
            .ok_or(RpcError::Truncated { part: "payload" })?;

        // Checked against the **uncompressed** length: a compressed segment's
        // own size says nothing about what it expands to, and a ceiling that
        // only looked at the bytes on the wire is no ceiling at all.
        if out.len().saturating_add(usize::from(header.size_actual)) > MAX_ROP_BUFFER {
            return Err(RpcError::TooLarge);
        }

        // Order matters and is not interchangeable: the obfuscation is applied
        // by the client to the *compressed* bytes, so it is undone first and
        // the result is what gets decompressed.
        let mut decoded = segment.to_vec();
        if header.is_obfuscated() {
            deobfuscate(&mut decoded);
        }
        if header.is_compressed() {
            decoded = crate::direct2::decompress(&decoded, usize::from(header.size_actual))?;
        }
        out.extend_from_slice(&decoded);

        if header.is_last() {
            return Ok(out);
        }
        input = input
            .get(body_end..)
            .ok_or(RpcError::Truncated { part: "payload" })?;
        // A chain that has consumed everything without a `Last` flag is
        // malformed, not merely finished.
        if input.is_empty() {
            return Err(RpcError::Unterminated);
        }
    }
}

/// Wraps a ROP payload in a single `Last` segment.
///
/// Written plain: neither obfuscated nor compressed. Obfuscation buys nothing —
/// the specification says so itself — and answering in the clear keeps a packet
/// capture readable when the next stage goes wrong.
///
/// # Errors
/// [`RpcError::TooLarge`] if the payload does not fit a segment's 16-bit length.
pub fn write_extended_buffer(payload: &[u8]) -> Result<Vec<u8>, RpcError> {
    let size = u16::try_from(payload.len()).map_err(|_| RpcError::TooLarge)?;
    let header = RpcHeaderExt {
        flags: FLAG_LAST,
        size,
        size_actual: size,
    };
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&header.to_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Builds one segment the way a client would.
    fn segment(payload: &[u8], flags: u16) -> Vec<u8> {
        let size = u16::try_from(payload.len()).unwrap();
        let header = RpcHeaderExt {
            flags,
            size,
            size_actual: size,
        };
        let mut out = header.to_bytes().to_vec();
        let mut body = payload.to_vec();
        if flags & FLAG_XOR_MAGIC != 0 {
            deobfuscate(&mut body);
        }
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn a_single_plain_segment_reads_back_unchanged() {
        let buffer = segment(b"rop-payload", FLAG_LAST);
        assert_eq!(read_extended_buffer(&buffer).unwrap(), b"rop-payload");
    }

    /// The obfuscation is an XOR with `0xA5` and is its own inverse — so a
    /// payload written obfuscated reads back as itself, and the bytes on the
    /// wire are genuinely not the plaintext.
    #[test]
    fn an_obfuscated_segment_is_xored_with_a5_and_reads_back() {
        let buffer = segment(b"rop-payload", FLAG_LAST | FLAG_XOR_MAGIC);
        assert_eq!(read_extended_buffer(&buffer).unwrap(), b"rop-payload");
        // The wire bytes are the plaintext XORed, byte for byte.
        let on_wire = &buffer[HEADER_LEN..];
        for (index, byte) in b"rop-payload".iter().enumerate() {
            assert_eq!(on_wire[index], byte ^ 0xA5);
        }
        assert_ne!(on_wire, b"rop-payload");
    }

    /// Segments chain until one carries `Last`, and the payloads concatenate
    /// in order. Mixed encodings across the chain are legal — each segment
    /// carries its own flags.
    #[test]
    fn segments_chain_until_one_says_it_is_last() {
        let mut buffer = segment(b"first-", 0);
        buffer.extend(segment(b"second-", FLAG_XOR_MAGIC));
        buffer.extend(segment(b"third", FLAG_LAST));
        assert_eq!(
            read_extended_buffer(&buffer).unwrap(),
            b"first-second-third"
        );
    }

    /// A chain that runs out without ever saying `Last` is malformed, not
    /// merely finished — treating it as finished would hand the ROP layer a
    /// half a request and let it act on it.
    #[test]
    fn a_chain_that_never_ends_is_an_error() {
        let buffer = segment(b"only", 0);
        assert_eq!(read_extended_buffer(&buffer), Err(RpcError::Unterminated));
    }

    /// A compressed segment is decompressed through to its payload.
    ///
    /// The stream here is hand-built rather than produced by a compressor we
    /// also wrote: a round trip through our own encoder would agree with itself
    /// even if both halves misread the specification. These are the bytes a
    /// client sends — a bitmask of all zeroes, then literals.
    #[test]
    fn a_compressed_segment_is_decompressed() {
        let payload = b"payload";
        let mut stream = Vec::new();
        // Seven literal bits (all zero), then the end-of-stream bit at bit 7.
        stream.extend_from_slice(&(1u32 << 7).to_le_bytes());
        stream.extend_from_slice(payload);

        let header = RpcHeaderExt {
            flags: FLAG_LAST | FLAG_COMPRESSED,
            size: u16::try_from(stream.len()).unwrap(),
            size_actual: u16::try_from(payload.len()).unwrap(),
        };
        let mut buffer = header.to_bytes().to_vec();
        buffer.extend_from_slice(&stream);

        assert_eq!(read_extended_buffer(&buffer).unwrap(), payload);
    }

    /// Compressed **and** obfuscated together, which is legal and is the case
    /// where the order matters: the client XORs the compressed bytes, so the
    /// XOR must be undone before decompression. Doing it the other way round
    /// produces rubbish rather than an error.
    #[test]
    fn a_segment_that_is_both_obfuscated_and_compressed_is_read_in_the_right_order() {
        let payload = b"payload";
        let mut stream = Vec::new();
        stream.extend_from_slice(&(1u32 << 7).to_le_bytes());
        stream.extend_from_slice(payload);

        let header = RpcHeaderExt {
            flags: FLAG_LAST | FLAG_COMPRESSED | FLAG_XOR_MAGIC,
            size: u16::try_from(stream.len()).unwrap(),
            size_actual: u16::try_from(payload.len()).unwrap(),
        };
        let mut buffer = header.to_bytes().to_vec();
        // The obfuscation is applied to the compressed bytes, as a client does.
        let mut obscured = stream.clone();
        deobfuscate(&mut obscured);
        buffer.extend_from_slice(&obscured);

        assert_eq!(read_extended_buffer(&buffer).unwrap(), payload);
    }

    #[test]
    fn the_version_the_spec_fixes_is_the_only_one_accepted() {
        let mut buffer = segment(b"payload", FLAG_LAST);
        buffer[0..2].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(
            read_extended_buffer(&buffer),
            Err(RpcError::Version { found: 1 })
        );
    }

    /// An uncompressed segment whose two lengths disagree contradicts the
    /// specification, so it is refused rather than reconciled by picking one.
    #[test]
    fn an_uncompressed_segment_must_agree_with_itself_about_its_length() {
        let mut buffer = segment(b"payload", FLAG_LAST);
        buffer[6..8].copy_from_slice(&99u16.to_le_bytes());
        assert_eq!(
            read_extended_buffer(&buffer),
            Err(RpcError::SizeMismatch {
                size: 7,
                actual: 99
            })
        );
    }

    /// A declared length longer than the bytes that arrived is the classic
    /// length-field lie, and it is checked rather than believed.
    #[test]
    fn a_segment_cannot_claim_more_payload_than_it_carries() {
        let mut buffer = segment(b"payload", FLAG_LAST);
        buffer[4..6].copy_from_slice(&5000u16.to_le_bytes());
        buffer[6..8].copy_from_slice(&5000u16.to_le_bytes());
        assert_eq!(
            read_extended_buffer(&buffer),
            Err(RpcError::Truncated { part: "payload" })
        );

        // ...and every truncation of a well-formed buffer is an error too,
        // never a silently short read.
        let good = segment(b"payload", FLAG_LAST);
        for cut in 1..good.len() {
            assert!(
                read_extended_buffer(&good[..cut]).is_err(),
                "accepted a buffer cut at {cut}"
            );
        }
    }

    #[test]
    fn an_empty_buffer_is_an_error() {
        assert_eq!(
            read_extended_buffer(&[]),
            Err(RpcError::Truncated { part: "header" })
        );
    }

    /// What we write, we can read: a round trip through the writer and reader
    /// returns the payload, and the segment we produce is marked `Last`.
    #[test]
    fn what_we_write_reads_back() {
        let payload = b"a ROP response".to_vec();
        let framed = write_extended_buffer(&payload).unwrap();
        let header = RpcHeaderExt::parse(&framed).unwrap();
        assert!(header.is_last());
        assert!(!header.is_compressed());
        assert!(!header.is_obfuscated(), "answered obscured for no benefit");
        assert_eq!(header.size, header.size_actual);
        assert_eq!(read_extended_buffer(&framed).unwrap(), payload);
    }

    /// A payload too large for a segment's 16-bit length is refused rather than
    /// silently truncated by the cast.
    #[test]
    fn a_payload_that_cannot_fit_a_segment_is_refused() {
        let huge = vec![0u8; usize::from(u16::MAX) + 1];
        assert_eq!(write_extended_buffer(&huge), Err(RpcError::TooLarge));
    }
}
