//! Reading a receipt that is **already in Drive** (alo Finance, ADR 0035, wave
//! B4.06b; `docs/design/finance.md`, "Expenses, receipts and mileage") — the one
//! tenant-scoped step between a file somebody uploaded and the candidate fields
//! [`crate::fin_receipt`] guesses from its text.
//!
//! # Why the receipt arrives as a node id and not as bytes
//!
//! A receipt is evidence: the claim that cites it keeps pointing at it for as
//! long as the books do, and the design note settles where it lives — *in Drive
//! under the claimant's own node, referenced by id, never copied into a finance
//! table*. A file posted to this module as bytes would therefore have to be put
//! *somewhere* by this module, which would make a second answer to "where do a
//! person's files live" and a second implementation of quota, naming and
//! permissions. So the upload is Drive's own (`POST /jmap/upload` +
//! `POST /drive/files`, the two calls every other attachment in the product
//! already uses), and what reaches finance is the node id.
//!
//! That choice is also what makes the isolation test here meaningful: the node
//! is read through [`AccountStore::drive_node`], the same door that answers the
//! Drive UI, so a colleague's private receipt and another tenant's are both
//! simply **absent** — and a claim can only ever cite a file its claimant could
//! already open.
//!
//! # Nothing is written, and nothing is decided
//!
//! This is a read: no row is inserted, no expense is created, no field is
//! stored. The answer is candidates for a human to confirm in the create form
//! ([`crate::fin_expenses`]), which is the design note's rejected-alternative in
//! reverse — *a draft in a list is a thing somebody approves without reading*.
//!
//! # A receipt is a file with somebody's life in it
//!
//! The bytes name a restaurant, a pharmacy, a city on a date. Nothing in this
//! module logs, the extracted text never reaches a column, and
//! [`ReceiptReading`] is handed straight back to the person who uploaded it.

use time::Date;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::extract::{extract_text, is_extractable};
use crate::fin_receipt::{ParsedReceipt, ReceiptInput, default_extractor};
use crate::id::{BlobId, DriveNodeId};

/// The largest receipt we will read, in bytes.
///
/// The same ceiling Drive's own content index uses: past it a file is not a
/// till roll or an invoice PDF, and pulling twelve megabytes out of the blob
/// store to look for the word "Summe" is work nobody asked for. Refused with a
/// message naming the limit rather than silently answered with nothing — a
/// person who photographed their receipt at full resolution can be told to
/// shrink it, but only if we say so.
pub const MAX_RECEIPT_BYTES: i64 = 12 * 1024 * 1024;

/// What one reading of a receipt found: the file it read, whether there was any
/// text in it at all, and the candidate fields.
#[derive(Debug, Clone)]
pub struct ReceiptReading {
    /// The Drive node that was read — echoed back so the caller can attach it
    /// to the claim it is about to create without holding state.
    pub node_id: DriveNodeId,
    /// The file's name in Drive, which is also what the extractor read as the
    /// filename (`REWE_2026-03-14.pdf` says two things).
    pub filename: String,
    /// What Drive holds as the file's media type, when it holds one.
    pub content_type: Option<String>,
    /// The file's size in bytes, as Drive records it.
    pub size: i64,
    /// Whether any text came out of the file at all.
    ///
    /// `false` is the ordinary answer for a photograph — a phone camera writes
    /// pixels, not characters — and it is the difference between "we read this
    /// and it says nothing" and "there was nothing here to read". A UI needs
    /// that difference to say something true to the person.
    pub had_text: bool,
    /// The candidates, every one of them optional and every one of them for a
    /// human to confirm.
    pub parsed: ParsedReceipt,
}

impl AccountStore {
    /// Reads a receipt the caller can already open in Drive, and returns the
    /// candidate fields for confirmation. **Writes nothing.**
    ///
    /// `today` is the day the reading happens on, passed in rather than read
    /// from a clock (see [`ReceiptInput::today`]): it buys exactly one rule, that
    /// a date in the future or a decade old is not when the money was spent.
    ///
    /// A file whose bytes hold no text — a photograph, an unknown binary — is
    /// **not an error**: the answer is [`ReceiptReading::had_text`] `false` with
    /// whatever the file's *name* gave up, and the person types the rest. That is
    /// the pre-B4.06 experience, one step shorter.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the node is not one this caller can read —
    /// another tenant's, a colleague's private file, or one that never existed;
    /// [`StoreError::Validation`] when the node holds no bytes (a folder) or is
    /// larger than [`MAX_RECEIPT_BYTES`]; [`StoreError::Blob`] when the blob
    /// store cannot be reached; [`StoreError::Db`] on failure.
    pub async fn read_receipt(&self, id: &DriveNodeId, today: Date) -> Result<ReceiptReading> {
        let node = self.drive_node(id).await?.ok_or(StoreError::NotFound)?;
        let Some(blob) = node.blob_id.clone() else {
            return Err(StoreError::Validation(
                "a receipt is a file: that Drive item holds no bytes to read".to_owned(),
            ));
        };
        if node.size > MAX_RECEIPT_BYTES {
            return Err(too_large());
        }
        let text = self
            .receipt_text(&node.kind, node.content_type.as_deref(), &blob)
            .await?;
        let parsed = default_extractor().extract(&ReceiptInput {
            text: text.as_deref().unwrap_or_default(),
            filename: Some(&node.name),
            today,
        });
        Ok(ReceiptReading {
            node_id: node.id,
            filename: node.name,
            content_type: node.content_type,
            size: node.size,
            had_text: text.is_some(),
            parsed,
        })
    }

    /// The text layer of a receipt's blob, or `None` when the file has none.
    ///
    /// Two guards and one rule. The guards: a media type we cannot read at all
    /// (an image) is not fetched, and the blob's **real** length is checked
    /// against [`MAX_RECEIPT_BYTES`] too — a node's `size` is a number the
    /// upload declared, so it is a claim about the file rather than the file.
    /// The rule: extraction runs on the blocking pool, because a slow or
    /// panicking parse of somebody's PDF must not stall the runtime — a dropped
    /// join is "no text", never a crash. [`crate::drive`]'s content index made
    /// the same call for the same reason.
    async fn receipt_text(
        &self,
        kind: &str,
        content_type: Option<&str>,
        blob: &str,
    ) -> Result<Option<String>> {
        if !is_extractable(kind, content_type) {
            return Ok(None);
        }
        let bytes = self
            .blob_bytes_for_send(&BlobId::new(blob.to_owned()))
            .await?;
        if i64::try_from(bytes.len()).unwrap_or(i64::MAX) > MAX_RECEIPT_BYTES {
            return Err(too_large());
        }
        let kind = kind.to_owned();
        let content_type = content_type.map(str::to_owned);
        Ok(tokio::task::spawn_blocking(move || {
            extract_text(&kind, content_type.as_deref(), &bytes)
        })
        .await
        .ok()
        .flatten())
    }
}

/// The one refusal both size checks share, naming the limit in the units a
/// person's file manager shows them.
fn too_large() -> StoreError {
    StoreError::Validation(format!(
        "a receipt must be smaller than {} MB",
        MAX_RECEIPT_BYTES / (1024 * 1024)
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_size_refusal_names_the_limit_a_person_can_act_on() {
        let StoreError::Validation(message) = too_large() else {
            panic!("a size refusal is a validation failure");
        };
        assert!(message.contains("12 MB"), "{message}");
    }
}
