//! Where a record came from (ADR 0058 §4) — provenance, one row per record.
//!
//! A record created out of something else carries its source: the quote an
//! invoice came from, the thread a task came from, the meeting a decision
//! came from. Modules set it where they create records; intents return it as
//! an `origin` field on the record view; agents cite it ("from the Northstar
//! thread") instead of asserting bare facts.
//!
//! **Set once, at creation.** Provenance is a fact about how a record began,
//! so the first writer wins and there is no update: the table's primary key
//! plus `ON CONFLICT DO NOTHING` make that structural. A creation path that
//! knows the true domain source (the accepted quote) writes first, inside
//! the creating call; the generic fallback (the chat thread an approved
//! proposal ran in) writes after and quietly loses to it.
//!
//! **A pointer, not a copy.** The row names the source's kind, id and the
//! label a person would cite it by; the source's content stays in its own
//! module's tables (constitution law #3), and the row outlives the source
//! the way `events` rows do — a deleted quote does not rewrite the history
//! of the invoice it once raised.
//!
//! Reads are tenant-scoped and nothing more: provenance is part of the
//! record, so whoever the module let read the record may read where it came
//! from. The module's own read path has already decided access by the time
//! this join happens.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::events::valid_event_name;

/// One record's source, as it is stored and as the record view renders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordOrigin {
    /// The source's kind (`quote`, `thread`, `meeting`, `email`) — same
    /// vocabulary as the event stream's record words.
    pub kind: String,
    /// The source's id, opaque to everyone but its own module.
    pub id: String,
    /// What a person would cite it by ("QUO-2026-00007", "#finance");
    /// `None` when the source has no name of its own (a bare DM).
    pub label: Option<String>,
    /// When the provenance was recorded — creation time, since it is set
    /// only then.
    pub created_at: OffsetDateTime,
}

/// The longest label kept. A label is a citation, not a document; a source
/// whose name is longer than this is cut at a character boundary rather
/// than refused, because losing the whole pointer over a long room name
/// would be the worse trade.
const MAX_LABEL: usize = 200;

/// Cut a label to [`MAX_LABEL`] bytes on a character boundary, and turn a
/// blank one into no label at all.
fn clean_label(label: Option<&str>) -> Option<String> {
    let trimmed = label?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut end = trimmed.len().min(MAX_LABEL);
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    Some(trimmed[..end].to_owned())
}

impl AccountStore {
    /// Record where a record came from, once.
    ///
    /// Returns whether this call was the one that set it — `false` means an
    /// earlier writer (the module's own creating call, or a concurrent one)
    /// already said where the record came from, and that answer stands.
    ///
    /// # Errors
    /// [`StoreError::Validation`] for a record type or origin kind outside
    /// the event vocabulary, or an id outside 1..=128 bytes;
    /// [`StoreError::Db`] on a database failure.
    pub async fn set_record_origin(
        &self,
        record_type: &str,
        record_id: &str,
        kind: &str,
        id: &str,
        label: Option<&str>,
    ) -> Result<bool> {
        for word in [record_type, kind] {
            if !valid_event_name(word) {
                return Err(StoreError::Validation(
                    "a record word must be lowercase words joined by '.' or '_'".to_owned(),
                ));
            }
        }
        for raw in [record_id, id] {
            if raw.is_empty() || raw.len() > 128 {
                return Err(StoreError::Validation(
                    "a record id must be 1..=128 bytes".to_owned(),
                ));
            }
        }
        let done = sqlx::query(
            "INSERT INTO record_origins \
                 (tenant_id, record_type, record_id, origin_kind, origin_id, origin_label) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (tenant_id, record_type, record_id) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(record_type)
        .bind(record_id)
        .bind(kind)
        .bind(id)
        .bind(clean_label(label))
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(done.rows_affected() == 1)
    }

    /// Where a record came from, when anything ever said so.
    ///
    /// Tenant-scoped and nothing more: by the time a record view asks this,
    /// the module's own read path has already decided the caller may see the
    /// record, and provenance is part of it. Another tenant's record id is
    /// not a different answer but an empty one.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn record_origin(
        &self,
        record_type: &str,
        record_id: &str,
    ) -> Result<Option<RecordOrigin>> {
        let row: Option<(String, String, Option<String>, OffsetDateTime)> = sqlx::query_as(
            "SELECT origin_kind, origin_id, origin_label, created_at \
             FROM record_origins \
             WHERE tenant_id = $1 AND record_type = $2 AND record_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(record_type)
        .bind(record_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(|(kind, id, label, created_at)| RecordOrigin {
            kind,
            id,
            label,
            created_at,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::clean_label;

    #[test]
    fn a_label_is_trimmed_cut_on_a_character_boundary_and_never_blank() {
        assert_eq!(
            clean_label(Some("  #finance  ")),
            Some("#finance".to_owned())
        );
        assert_eq!(clean_label(Some("   ")), None);
        assert_eq!(clean_label(None), None);
        // 210 two-byte characters: the cut lands inside a character and steps
        // back to the boundary rather than slicing mid-codepoint.
        let long = "é".repeat(210);
        let kept = clean_label(Some(&long)).unwrap();
        assert!(kept.len() <= super::MAX_LABEL);
        assert!(kept.chars().all(|c| c == 'é'));
    }
}
