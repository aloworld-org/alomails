//! `GLOBSET` and `IDSET` ([MS-OXCFXICS] §2.2.2.4–2.2.2.6) — the compressed set
//! of identifiers that tells client and server what each already knows.
//!
//! Incremental synchronisation rests on one idea: instead of naming every
//! message it holds, a client sends a *set* of identifiers, and the server
//! replies with what changed outside that set. Those sets are the difference
//! between cached mode costing one round trip and costing a mailbox.
//!
//! A `GLOBCNT` is the 6-byte counter half of a folder or message id
//! ([MS-OXCFXICS] §2.2.2.5). A `GLOBSET` is a set of them, reduced to ranges
//! and then compressed with a tiny stack machine.
//!
//! ## The byte order is the trap
//!
//! The stack holds the **high-order** bytes shared by neighbouring values, so
//! for every purpose in this module a `GLOBCNT` is six bytes **most significant
//! first**. That is the opposite of how the same counter sits inside a folder
//! id on the wire, where it is little-endian ([MS-OXCDATA] §2.2.1.1).
//!
//! Get it backwards and nothing errors: the ranges still encode, still decode,
//! and describe a completely different set of messages — so the client silently
//! synchronises the wrong mail. [`globcnt_to_bytes`] and [`globcnt_from_bytes`]
//! are the only two places that conversion happens.
//!
//! ## The commands
//!
//! | Command | Byte | Effect |
//! |---|---|---|
//! | Push | `0x01`–`0x06` | push that many shared high-order bytes |
//! | Pop | `0x50` | remove the most recent push |
//! | Bitmask | `0x42` | up to five ranges within eight of each other |
//! | Range | `0x52` | one range, low then high |
//! | End | `0x00` | terminates the `GLOBSET` |
//!
//! A push that brings the stack to six bytes is special: it *is* a singleton
//! range, and its bytes come off again automatically without a `Pop`
//! ([MS-OXCFXICS] §3.1.5.4.3.1.1).
//!
//! We **write** only push, range and end — bitmask is a `SHOULD`-level
//! optimisation, and a set we cannot produce is a set we cannot get wrong. We
//! **read** all five, because a real client uses whatever it likes: strict in
//! what we send, tolerant in what we accept.

use thiserror::Error;

/// The Push command's lowest byte ([MS-OXCFXICS] §2.2.2.6.1).
const CMD_PUSH_MIN: u8 = 0x01;
/// The Push command's highest byte.
const CMD_PUSH_MAX: u8 = 0x06;
/// The Pop command ([MS-OXCFXICS] §2.2.2.6.2).
const CMD_POP: u8 = 0x50;
/// The Bitmask command ([MS-OXCFXICS] §2.2.2.6.3).
const CMD_BITMASK: u8 = 0x42;
/// The Range command ([MS-OXCFXICS] §2.2.2.6.4).
const CMD_RANGE: u8 = 0x52;
/// The End command ([MS-OXCFXICS] §2.2.2.6.5).
const CMD_END: u8 = 0x00;

/// A `GLOBCNT` is six bytes.
const GLOBCNT_LEN: usize = 6;

/// The largest value a 48-bit counter can hold.
pub const GLOBCNT_MAX: u64 = (1 << 48) - 1;

/// The stack depth the Bitmask command requires ([MS-OXCFXICS] §3.1.5.4.3.2.3).
const BITMASK_STACK_DEPTH: usize = 5;

/// A bound on how many ranges we will decode from one client-supplied set.
///
/// The input is attacker-influenced — it arrives in an upload-state stream — so
/// the decode is bounded rather than trusted. A real mailbox produces a handful
/// of ranges; anything approaching this is malformed or hostile.
pub const MAX_RANGES: usize = 32_768;

/// What can go wrong decoding a `GLOBSET`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum IcsError {
    /// The buffer ended mid-command.
    #[error("GLOBSET truncated in {part}")]
    Truncated {
        /// Which part ran out.
        part: &'static str,
    },
    /// A command byte that is not one of the five.
    #[error("unknown GLOBSET command {command:#04X}")]
    UnknownCommand {
        /// The byte that was not understood.
        command: u8,
    },
    /// A Pop with nothing pushed, or a Push past six bytes.
    #[error("GLOBSET stack underflow or overflow at depth {depth}")]
    Stack {
        /// The depth when the command arrived.
        depth: usize,
    },
    /// Bitmask arrived at a stack depth other than five.
    ///
    /// [MS-OXCFXICS] §3.1.5.4.3.2.3 names the error to return for this exact
    /// case, so it is reported rather than guessed at.
    #[error("Bitmask command needs a stack of 5 bytes, found {depth}")]
    BitmaskStack {
        /// The depth when Bitmask arrived.
        depth: usize,
    },
    /// A range whose low value exceeds its high value.
    #[error("GLOBSET range {low:#014X}..{high:#014X} is inverted")]
    InvertedRange {
        /// The low value.
        low: u64,
        /// The high value.
        high: u64,
    },
    /// More ranges than [`MAX_RANGES`].
    #[error("GLOBSET declares more than {MAX_RANGES} ranges")]
    TooManyRanges,
}

/// A run of consecutive `GLOBCNT` values, inclusive at both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GlobRange {
    /// The lowest value in the run.
    pub low: u64,
    /// The highest value in the run.
    pub high: u64,
}

impl GlobRange {
    /// A run covering exactly one value.
    #[must_use]
    pub fn single(value: u64) -> Self {
        Self {
            low: value,
            high: value,
        }
    }

    /// A run from `low` to `high` inclusive.
    #[must_use]
    pub fn new(low: u64, high: u64) -> Self {
        Self { low, high }
    }

    /// Whether `value` falls in this run.
    #[must_use]
    pub fn contains(&self, value: u64) -> bool {
        (self.low..=self.high).contains(&value)
    }
}

/// The six bytes of a `GLOBCNT`, most significant first.
///
/// See the module documentation: this order is the opposite of the counter's
/// order inside a folder id, and the two must never be confused.
#[must_use]
pub fn globcnt_to_bytes(value: u64) -> [u8; GLOBCNT_LEN] {
    let full = value.to_be_bytes();
    [full[2], full[3], full[4], full[5], full[6], full[7]]
}

/// Reassembles a `GLOBCNT` from six most-significant-first bytes.
#[must_use]
pub fn globcnt_from_bytes(bytes: [u8; GLOBCNT_LEN]) -> u64 {
    let mut value = 0_u64;
    for byte in bytes {
        value = (value << 8) | u64::from(byte);
    }
    value
}

/// Sorts and merges runs so that none overlap or touch.
///
/// Two runs that merely abut (`..=5` and `6..=`) become one: leaving them apart
/// would encode the same set as more commands, and would make two encodings of
/// one set, which makes the round-trip tests meaningless.
#[must_use]
pub fn normalize(ranges: &[GlobRange]) -> Vec<GlobRange> {
    let mut sorted: Vec<GlobRange> = ranges.iter().copied().filter(|r| r.low <= r.high).collect();
    sorted.sort_unstable();

    let mut merged: Vec<GlobRange> = Vec::with_capacity(sorted.len());
    for range in sorted {
        match merged.last_mut() {
            // `saturating_add` so a run ending at the 48-bit ceiling cannot
            // wrap and swallow the whole set.
            Some(last) if range.low <= last.high.saturating_add(1) => {
                last.high = last.high.max(range.high);
            }
            _ => merged.push(range),
        }
    }
    merged
}

/// Serialises runs into a `GLOBSET` ([MS-OXCFXICS] §3.1.5.4.3.1).
///
/// Shares high-order bytes between neighbouring runs via the stack, emits each
/// run as a Range — or, where a run is a single value, as the six-byte Push
/// that means the same thing in fewer bytes — and terminates with End.
#[must_use]
pub fn serialize_globset(ranges: &[GlobRange]) -> Vec<u8> {
    let ranges = normalize(ranges);
    let mut out = Vec::new();
    // The bytes currently on the stack, and the size of each push that put
    // them there — Pop removes one whole push, not one byte.
    let mut stack: Vec<u8> = Vec::with_capacity(GLOBCNT_LEN);
    let mut groups: Vec<usize> = Vec::new();

    for range in ranges {
        let low = globcnt_to_bytes(range.low);
        let high = globcnt_to_bytes(range.high);
        let common = shared_prefix(&low, &high);

        // Unwind until what remains is a prefix of the bytes this run shares.
        while !low[..common].starts_with(&stack) {
            let Some(size) = groups.pop() else { break };
            stack.truncate(stack.len() - size);
            out.push(CMD_POP);
        }

        if stack.len() < common {
            let pushed = &low[stack.len()..common];
            out.push(u8::try_from(pushed.len()).unwrap_or(CMD_PUSH_MAX));
            out.extend_from_slice(pushed);

            if common == GLOBCNT_LEN {
                // A push that fills the stack *is* the singleton range, and its
                // bytes come off again by themselves — no Pop, no Range.
                continue;
            }
            stack.extend_from_slice(pushed);
            groups.push(pushed.len());
        }

        out.push(CMD_RANGE);
        out.extend_from_slice(&low[stack.len()..]);
        out.extend_from_slice(&high[stack.len()..]);
    }

    out.push(CMD_END);
    out
}

/// Parses a `GLOBSET` back into runs ([MS-OXCFXICS] §3.1.5.4.3.2).
///
/// Accepts all five commands, including the Bitmask this module never writes.
///
/// # Errors
///
/// Returns [`IcsError`] for a truncated buffer, an unknown command, a stack
/// misuse, an inverted range, or more runs than [`MAX_RANGES`].
pub fn parse_globset(bytes: &[u8]) -> Result<Vec<GlobRange>, IcsError> {
    let mut ranges: Vec<GlobRange> = Vec::new();
    let mut stack: Vec<u8> = Vec::with_capacity(GLOBCNT_LEN);
    let mut groups: Vec<usize> = Vec::new();
    let mut at = 0_usize;

    loop {
        let Some(&command) = bytes.get(at) else {
            // A set that simply stops is treated as ended: some clients omit
            // the final End, and refusing the whole state over a missing
            // terminator would break synchronisation for a trailing byte.
            return Ok(ranges);
        };
        at += 1;

        match command {
            CMD_END => return Ok(ranges),

            CMD_PUSH_MIN..=CMD_PUSH_MAX => {
                let count = usize::from(command);
                if stack.len() + count > GLOBCNT_LEN {
                    return Err(IcsError::Stack { depth: stack.len() });
                }
                let taken = bytes
                    .get(at..at + count)
                    .ok_or(IcsError::Truncated { part: "Push bytes" })?;
                at += count;
                stack.extend_from_slice(taken);

                if stack.len() == GLOBCNT_LEN {
                    // Filling the stack is a singleton, and unwinds itself.
                    let mut value = [0_u8; GLOBCNT_LEN];
                    value.copy_from_slice(&stack);
                    push_range(&mut ranges, GlobRange::single(globcnt_from_bytes(value)))?;
                    stack.truncate(GLOBCNT_LEN - count);
                } else {
                    groups.push(count);
                }
            }

            CMD_POP => {
                let size = groups.pop().ok_or(IcsError::Stack { depth: 0 })?;
                stack.truncate(stack.len() - size);
            }

            CMD_RANGE => {
                let width = GLOBCNT_LEN - stack.len();
                let low = take_value(bytes, &mut at, &stack, width, "Range low")?;
                let high = take_value(bytes, &mut at, &stack, width, "Range high")?;
                if low > high {
                    return Err(IcsError::InvertedRange { low, high });
                }
                push_range(&mut ranges, GlobRange::new(low, high))?;
            }

            CMD_BITMASK => {
                if stack.len() != BITMASK_STACK_DEPTH {
                    return Err(IcsError::BitmaskStack { depth: stack.len() });
                }
                let start = *bytes.get(at).ok_or(IcsError::Truncated {
                    part: "Bitmask StartingValue",
                })?;
                let mask = *bytes.get(at + 1).ok_or(IcsError::Truncated {
                    part: "Bitmask field",
                })?;
                at += 2;

                // The StartingValue is always present; bit N stands for
                // StartingValue + 1 + N ([MS-OXCFXICS] §3.1.5.4.3.1.3).
                let mut low_bytes = vec![start];
                for bit in 0..8_u8 {
                    if mask & (1 << bit) != 0 {
                        // Values beyond 0xFF cannot be expressed by this
                        // command, so a bit implying one is malformed input.
                        if let Some(value) = start.checked_add(bit + 1) {
                            low_bytes.push(value);
                        }
                    }
                }

                let mut whole = [0_u8; GLOBCNT_LEN];
                whole[..BITMASK_STACK_DEPTH].copy_from_slice(&stack);
                for low in collapse(&low_bytes) {
                    whole[BITMASK_STACK_DEPTH] = low.0;
                    let from = globcnt_from_bytes(whole);
                    whole[BITMASK_STACK_DEPTH] = low.1;
                    let to = globcnt_from_bytes(whole);
                    push_range(&mut ranges, GlobRange::new(from, to))?;
                }
            }

            other => return Err(IcsError::UnknownCommand { command: other }),
        }
    }
}

/// Reads one value: the stack's bytes followed by `width` from the buffer.
fn take_value(
    bytes: &[u8],
    at: &mut usize,
    stack: &[u8],
    width: usize,
    part: &'static str,
) -> Result<u64, IcsError> {
    let taken = bytes
        .get(*at..*at + width)
        .ok_or(IcsError::Truncated { part })?;
    *at += width;

    let mut whole = [0_u8; GLOBCNT_LEN];
    whole[..stack.len()].copy_from_slice(stack);
    whole[stack.len()..].copy_from_slice(taken);
    Ok(globcnt_from_bytes(whole))
}

/// Appends a run, refusing to grow without bound.
fn push_range(ranges: &mut Vec<GlobRange>, range: GlobRange) -> Result<(), IcsError> {
    if ranges.len() >= MAX_RANGES {
        return Err(IcsError::TooManyRanges);
    }
    ranges.push(range);
    Ok(())
}

/// Collapses sorted single bytes into inclusive runs.
fn collapse(values: &[u8]) -> Vec<(u8, u8)> {
    let mut out: Vec<(u8, u8)> = Vec::new();
    for &value in values {
        match out.last_mut() {
            Some(last) if value == last.1.saturating_add(1) => last.1 = value,
            _ => out.push((value, value)),
        }
    }
    out
}

/// How many leading bytes two values share.
fn shared_prefix(left: &[u8; GLOBCNT_LEN], right: &[u8; GLOBCNT_LEN]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(a, b)| a == b)
        .count()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Most significant byte first — the opposite of the counter's order inside
    /// a folder id, and the difference between synchronising the right mail and
    /// the wrong mail.
    #[test]
    fn globcnt_bytes_are_most_significant_first() {
        assert_eq!(
            globcnt_to_bytes(0x0000_0000_0001),
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x01]
        );
        assert_eq!(
            globcnt_to_bytes(0x0102_0304_0506),
            [0x01, 0x02, 0x03, 0x04, 0x05, 0x06]
        );
        assert_eq!(
            globcnt_from_bytes([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]),
            0x0102_0304_0506
        );
        assert_eq!(
            globcnt_from_bytes(globcnt_to_bytes(GLOBCNT_MAX)),
            GLOBCNT_MAX
        );
    }

    /// The worked example from [MS-OXCFXICS] §3.1.5.4.3.1.3, used as the
    /// specification's own test vector: five common high bytes, StartingValue
    /// 0x01 and Bitmask 0xEB must yield {0x01-0x03, 0x05-0x05, 0x07-0x09}.
    #[test]
    fn bitmask_decodes_the_specifications_own_example() {
        let mut buf = vec![0x05, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        buf.extend_from_slice(&[CMD_BITMASK, 0x01, 0xEB, CMD_END]);

        let ranges = parse_globset(&buf).unwrap();
        let base = 0xAABB_CCDD_EE00_u64;
        assert_eq!(
            ranges,
            vec![
                GlobRange::new(base + 0x01, base + 0x03),
                GlobRange::single(base + 0x05),
                GlobRange::new(base + 0x07, base + 0x09),
            ]
        );
    }

    /// Bitmask at any depth but five is the error the specification names, not
    /// a best guess at what the client meant.
    #[test]
    fn bitmask_at_the_wrong_stack_depth_is_refused() {
        let buf = vec![0x02, 0xAA, 0xBB, CMD_BITMASK, 0x01, 0xEB, CMD_END];
        assert_eq!(
            parse_globset(&buf),
            Err(IcsError::BitmaskStack { depth: 2 })
        );
    }

    /// A push that fills the stack is a singleton and unwinds itself — no Pop.
    #[test]
    fn six_byte_push_is_a_singleton_that_unwinds_itself() {
        let buf = vec![0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2A, CMD_END];
        assert_eq!(parse_globset(&buf).unwrap(), vec![GlobRange::single(0x2A)]);

        // And the encoder chooses that form for a single value.
        let encoded = serialize_globset(&[GlobRange::single(0x2A)]);
        assert_eq!(encoded, buf);
    }

    /// With no byte in common, a range carries both values in full and the
    /// stack stays empty.
    #[test]
    fn a_range_with_no_shared_bytes_writes_both_values_whole() {
        // The top bytes differ (0x00 against 0x01), so nothing can be shared.
        let range = GlobRange::new(0x0000_0000_0001, 0x0100_0000_0000);
        let encoded = serialize_globset(&[range]);
        assert_eq!(
            encoded,
            vec![
                CMD_RANGE, //
                0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // low, all six bytes
                0x01, 0x00, 0x00, 0x00, 0x00, 0x00, // high, all six bytes
                CMD_END,
            ]
        );
        assert_eq!(parse_globset(&encoded).unwrap(), vec![range]);
    }

    /// Where the values *do* share leading bytes, those bytes are pushed once
    /// and each end of the range carries only what is left.
    #[test]
    fn a_range_pushes_the_bytes_its_ends_share() {
        // Both ends begin 0x00; they diverge at the second byte.
        let range = GlobRange::new(0x01, 0x00FF_0000_0000);
        let encoded = serialize_globset(&[range]);
        assert_eq!(
            encoded,
            vec![
                0x01, 0x00,      // push the one shared byte
                CMD_RANGE, //
                0x00, 0x00, 0x00, 0x00, 0x01, // low, five bytes left
                0xFF, 0x00, 0x00, 0x00, 0x00, // high, five bytes left
                CMD_END,
            ]
        );
        assert_eq!(parse_globset(&encoded).unwrap(), vec![range]);
    }

    /// Touching and overlapping runs collapse, so one set has one encoding.
    #[test]
    fn normalize_merges_touching_and_overlapping_runs() {
        let merged = normalize(&[
            GlobRange::new(10, 12),
            GlobRange::new(13, 15), // abuts
            GlobRange::new(14, 20), // overlaps
            GlobRange::new(30, 31),
            GlobRange::new(5, 6),
        ]);
        assert_eq!(
            merged,
            vec![
                GlobRange::new(5, 6),
                GlobRange::new(10, 20),
                GlobRange::new(30, 31)
            ]
        );
    }

    /// A run at the 48-bit ceiling must not wrap while merging.
    #[test]
    fn normalize_does_not_wrap_at_the_ceiling() {
        let merged = normalize(&[GlobRange::single(GLOBCNT_MAX), GlobRange::single(1)]);
        assert_eq!(
            merged,
            vec![GlobRange::single(1), GlobRange::single(GLOBCNT_MAX)]
        );
    }

    /// The property that matters: whatever we write, we read back unchanged.
    /// A deterministic generator stands in for a fuzzer — the encoder's stack
    /// bookkeeping is where an off-by-one hides, and only many shapes find it.
    #[test]
    fn every_set_survives_a_round_trip() {
        // A small LCG: reproducible, and no dependency.
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        for case in 0..400 {
            let count = usize::try_from(next() % 12).unwrap_or(0) + 1;
            let mut ranges = Vec::new();
            for _ in 0..count {
                // Mix wide-apart values with tightly clustered ones, so both
                // the shared-prefix path and the no-shared-bytes path are hit.
                let base = if case % 3 == 0 {
                    next() % 512
                } else {
                    next() & GLOBCNT_MAX
                };
                let span = next() % 8;
                ranges.push(GlobRange::new(
                    base,
                    base.saturating_add(span).min(GLOBCNT_MAX),
                ));
            }

            let expected = normalize(&ranges);
            let encoded = serialize_globset(&ranges);
            let decoded = parse_globset(&encoded).unwrap();
            assert_eq!(
                normalize(&decoded),
                expected,
                "case {case} did not survive: {ranges:?} encoded to {encoded:02X?}"
            );
        }
    }

    /// Client-supplied bytes are bounded and checked, never trusted.
    #[test]
    fn malformed_input_is_refused_rather_than_guessed() {
        assert_eq!(
            parse_globset(&[0x03, 0xAA]),
            Err(IcsError::Truncated { part: "Push bytes" })
        );
        assert_eq!(
            parse_globset(&[CMD_POP, CMD_END]),
            Err(IcsError::Stack { depth: 0 })
        );
        assert_eq!(
            parse_globset(&[0x77, CMD_END]),
            Err(IcsError::UnknownCommand { command: 0x77 })
        );
        // A range whose low exceeds its high.
        let inverted = vec![
            CMD_RANGE, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, // low 9
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // high 1
            CMD_END,
        ];
        assert!(matches!(
            parse_globset(&inverted),
            Err(IcsError::InvertedRange { low: 9, high: 1 })
        ));
        // Pushing past six bytes.
        assert_eq!(
            parse_globset(&[0x04, 1, 2, 3, 4, 0x04, 5, 6, 7, 8]),
            Err(IcsError::Stack { depth: 4 })
        );
    }

    /// An empty set is still a well-formed one.
    #[test]
    fn an_empty_set_is_just_the_end_command() {
        assert_eq!(serialize_globset(&[]), vec![CMD_END]);
        assert_eq!(parse_globset(&[CMD_END]).unwrap(), Vec::new());
        assert_eq!(parse_globset(&[]).unwrap(), Vec::new());
    }
}
