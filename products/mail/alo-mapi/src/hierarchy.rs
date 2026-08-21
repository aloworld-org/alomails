//! `RopGetHierarchyTable` ([MS-OXCROPS] §2.2.4.13, [MS-OXCFOLD] §2.2.1.13) —
//! asking a folder for a table of its children.
//!
//! Request, 5 bytes:
//!
//! | Field | Size | |
//! |---|---|---|
//! | `RopId` | 1 | `0x04` |
//! | `LogonId` | 1 | |
//! | `InputHandleIndex` | 1 | the folder whose children are wanted |
//! | `OutputHandleIndex` | 1 | where the table's handle goes |
//! | `TableFlags` | 1 | |
//!
//! Success response, 10 bytes:
//!
//! | Field | Size |
//! |---|---|
//! | `RopId` | 1 |
//! | `OutputHandleIndex` | 1 |
//! | `ReturnValue` | 4 |
//! | `RowCount` | 4 |
//!
//! ## What the hierarchy actually is
//!
//! `RowCount` is the first number in this adapter that has to be *true about a
//! mailbox* rather than true about the protocol, and it is worth being exact
//! about what it currently reports.
//!
//! The logon advertises thirteen special folders, and their parent/child shape
//! is fixed by what those folders mean: the mailbox root holds the
//! interpersonal-messages subtree and the folders a user never sees, and the
//! subtree holds the four a user does. That structure is real — it is the same
//! for every mailbox alo serves — so reporting it is not a guess.
//!
//! What is **not** here yet is the user's own folders: the ones they made
//! themselves live in the JMAP store and are not part of this fixed set. Until
//! the stage that reads them, a client sees the standard folders and no others.
//! That is a smaller mailbox than the truth, which is why it is written down
//! rather than left for somebody to discover.

use crate::logon_response::SpecialFolder;
use crate::rop::RopError;

/// The `RopId` of `RopGetHierarchyTable`.
pub const ROP_GET_HIERARCHY_TABLE: u8 = 0x04;

/// The fixed size of this request.
pub const REQUEST_LEN: usize = 5;

/// The size of a success response.
pub const RESPONSE_LEN: usize = 10;

/// A parsed `RopGetHierarchyTable` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HierarchyTableRequest {
    /// The logon this operation belongs to.
    pub logon_id: u8,
    /// The handle-table slot holding the folder whose children are wanted.
    pub input_handle_index: u8,
    /// The handle-table slot the table's handle goes into.
    pub output_handle_index: u8,
    /// Flags controlling the kind of table.
    pub table_flags: u8,
}

impl HierarchyTableRequest {
    /// Parses the request and returns it with the bytes that follow.
    ///
    /// # Errors
    /// [`RopError::Truncated`] if fewer than [`REQUEST_LEN`] bytes remain, or
    /// the leading byte is not this operation.
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), RopError> {
        let fixed = input.get(..REQUEST_LEN).ok_or(RopError::Truncated {
            part: "RopGetHierarchyTable",
        })?;
        if fixed[0] != ROP_GET_HIERARCHY_TABLE {
            return Err(RopError::Truncated {
                part: "RopGetHierarchyTable",
            });
        }
        Ok((
            Self {
                logon_id: fixed[1],
                input_handle_index: fixed[2],
                output_handle_index: fixed[3],
                table_flags: fixed[4],
            },
            &input[REQUEST_LEN..],
        ))
    }
}

/// The children of a special folder.
///
/// Fixed, because these folders' relationships are fixed: every mailbox alo
/// serves has exactly this shape at the top. A user's own folders are not here
/// — see the module note.
#[must_use]
pub fn children(folder: SpecialFolder) -> &'static [SpecialFolder] {
    use SpecialFolder as F;
    match folder {
        // The mailbox root holds the user-visible subtree and the folders that
        // exist for the protocol's benefit rather than the reader's.
        F::Root => &[
            F::IpmSubtree,
            F::DeferredAction,
            F::SpoolerQueue,
            F::CommonViews,
            F::Schedule,
            F::Search,
            F::Views,
            F::Shortcuts,
        ],
        // What a person actually sees when they open their mail.
        F::IpmSubtree => &[F::Inbox, F::Outbox, F::SentItems, F::DeletedItems],
        // Everything else is a leaf until the store's own folders arrive.
        _ => &[],
    }
}

/// Builds a `RopGetHierarchyTable` success response.
#[must_use]
pub fn success_body(output_handle_index: u8, row_count: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(RESPONSE_LEN);
    out.push(ROP_GET_HIERARCHY_TABLE);
    out.push(output_handle_index);
    out.extend_from_slice(&0u32.to_le_bytes()); // ReturnValue: success.
    out.extend_from_slice(&row_count.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn request() -> Vec<u8> {
        vec![ROP_GET_HIERARCHY_TABLE, 0x00, 0x01, 0x02, 0x00]
    }

    #[test]
    fn a_request_reads_back_field_for_field() {
        let raw = request();
        let (table, rest) = HierarchyTableRequest::parse(&raw).unwrap();
        assert_eq!(table.logon_id, 0);
        assert_eq!(table.input_handle_index, 1);
        assert_eq!(table.output_handle_index, 2);
        assert_eq!(table.table_flags, 0);
        assert!(rest.is_empty());
    }

    #[test]
    fn every_truncation_is_an_error() {
        let full = request();
        for cut in 0..full.len() {
            assert!(
                HierarchyTableRequest::parse(&full[..cut]).is_err(),
                "accepted a request cut at {cut}"
            );
        }
    }

    #[test]
    fn another_rop_id_is_not_a_hierarchy_table() {
        let mut raw = request();
        raw[0] = 0x02;
        assert!(HierarchyTableRequest::parse(&raw).is_err());
    }

    #[test]
    fn a_success_response_is_ten_bytes() {
        let body = success_body(2, 4);
        assert_eq!(body.len(), RESPONSE_LEN);
        assert_eq!(body[0], ROP_GET_HIERARCHY_TABLE);
        assert_eq!(body[1], 2, "the request's output index is echoed");
        assert_eq!(&body[2..6], &0u32.to_le_bytes(), "ReturnValue");
        assert_eq!(&body[6..10], &4u32.to_le_bytes(), "RowCount");
    }

    /// The shape a client will draw. Stated as an assertion so that changing it
    /// is a deliberate act rather than a side effect of editing a match arm.
    #[test]
    fn the_hierarchy_is_the_shape_every_mailbox_has() {
        use SpecialFolder as F;

        // What a person sees.
        assert_eq!(
            children(F::IpmSubtree),
            &[F::Inbox, F::Outbox, F::SentItems, F::DeletedItems]
        );

        // The root holds the subtree plus the folders kept for the protocol.
        let root = children(F::Root);
        assert!(root.contains(&F::IpmSubtree));
        assert!(!root.contains(&F::Inbox), "Inbox hangs off the subtree");
        assert!(!root.contains(&F::Root), "the root is not its own child");

        // Every folder appears at most once across the whole hierarchy, and the
        // root is the only one with no parent — a cycle or a duplicate here is
        // a folder tree a client cannot draw.
        let mut seen = Vec::new();
        for folder in F::ALL {
            for child in children(folder) {
                assert!(!seen.contains(child), "{child:?} has more than one parent");
                seen.push(*child);
            }
        }
        assert!(!seen.contains(&F::Root), "the root has a parent");
        assert_eq!(seen.len(), F::ALL.len() - 1, "a folder has no parent");
    }

    /// Leaves report no children rather than an error: an empty folder is a
    /// perfectly good answer, and the four a user reads are leaves today.
    #[test]
    fn the_folders_a_user_reads_are_leaves_for_now() {
        for folder in [
            SpecialFolder::Inbox,
            SpecialFolder::SentItems,
            SpecialFolder::DeletedItems,
            SpecialFolder::Outbox,
        ] {
            assert!(children(folder).is_empty(), "{folder:?}");
        }
    }
}
