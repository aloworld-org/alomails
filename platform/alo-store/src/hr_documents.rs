//! The papers on a person's file — contracts, amendments, letters (alo HR,
//! ADR 0035, wave B6.02b; `docs/design/hr.md`, "Routes").
//!
//! A document is **a Drive node filed against an employee**, never bytes of its
//! own. One file tree, one version history, one blob store, one download path:
//! a second file store inside HR would be a second place a contract can be,
//! with different access rules, which is the failure this module exists to
//! prevent rather than to add.
//!
//! # The node must be in the tenant's HR area
//!
//! [`DriveLocation::Hr`](crate::DriveLocation::Hr) is a Drive location whose
//! read *and* write gate is the HR role (or a tenant admin), and this store
//! refuses to file anything else — a node in somebody's personal files, in a
//! Space, or in another tenant is the same [`StoreError::NotFound`] the design
//! note's error table names.
//!
//! That refusal is what makes the promise structural rather than a habit of
//! this file: because the node lives in the HR area, the ordinary Drive
//! download path already refuses the colleague who guesses its id. A filing row
//! that pointed at a node in a Space would be a record saying "HR-only" over a
//! file anybody in that Space could open.
//!
//! # What is not here
//!
//! The document's **contents**. We do not read, parse, summarise or index an
//! employment contract or a sick note; the row says which file it is, what kind
//! of paper it is, who filed it and when. `note` is the filer's word for *which
//! paper* ("addendum, four-day week"), and the same rule the audit trail has
//! applies to it: nothing here is a field value about the person.

use time::OffsetDateTime;

use crate::drive::HR_AREA;
use crate::error::{Result, StoreError};
use crate::id::{DriveNodeId, HrDocumentId, HrEmployeeId, UserId};
use crate::store::TenantStore;

/// Longest filing note. Long enough for "addendum 2, four-day week from
/// October"; short enough that nobody mistakes it for a place to write about a
/// person.
pub const DOCUMENT_NOTE_MAX_CHARS: usize = 200;

/// What kind of paper a filed document is.
///
/// A closed vocabulary, like every other in this suite: a word no code knows is
/// a category nothing can report on, and the database's own CHECK says the same
/// thing one layer down. Widening it is a design change
/// (`docs/design/hr.md`), not a schema tweak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HrDocumentKind {
    /// The employment contract itself.
    Contract,
    /// A change to the terms — an addendum, a new working pattern, a pay
    /// letter — the paper behind an appended employment.
    Amendment,
    /// A letter to or about the person: an employment confirmation, a
    /// reference, a warning.
    Letter,
    /// A certificate the employer must hold: a qualification, a licence, a
    /// medical certificate handed in for an absence.
    Certificate,
    /// Anything else on the file. Deliberately last and deliberately vague —
    /// the alternative is a tenant inventing a spelling of "contract".
    Other,
}

impl HrDocumentKind {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contract => "contract",
            Self::Amendment => "amendment",
            Self::Letter => "letter",
            Self::Certificate => "certificate",
            Self::Other => "other",
        }
    }

    /// Reads a kind from a request body or a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set — the caller can fix
    /// it, and the list is short enough to be the whole message.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "contract" => Ok(Self::Contract),
            "amendment" => Ok(Self::Amendment),
            "letter" => Ok(Self::Letter),
            "certificate" => Ok(Self::Certificate),
            "other" => Ok(Self::Other),
            _ => Err(StoreError::Validation(
                "document kind must be one of: contract, amendment, letter, certificate, other"
                    .to_owned(),
            )),
        }
    }
}

impl std::fmt::Display for HrDocumentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One paper on a person's file, as HR reads it.
///
/// The file's own name and size come from the Drive node in the same read, so a
/// list of somebody's documents is one round trip and a client never has to ask
/// Drive a second question it might be refused. They are `None` when the node
/// has since been purged through Drive's trash — the filing stays as the record
/// that a paper was once here, which is the honest answer to "where is the
/// contract?".
#[derive(Debug, Clone)]
pub struct HrDocument {
    /// The filing.
    pub id: HrDocumentId,
    /// Whose file it is on.
    pub employee_id: HrEmployeeId,
    /// The Drive node holding the file, in the tenant's HR area.
    pub node_id: DriveNodeId,
    /// What kind of paper it is.
    pub kind: HrDocumentKind,
    /// The filer's word for which paper this is.
    pub note: String,
    /// The file's name in Drive, when the node is still there.
    pub file_name: Option<String>,
    /// Its size in bytes, when the node is still there.
    pub size: Option<i64>,
    /// Its content type, when Drive recorded one.
    pub content_type: Option<String>,
    /// Whether the node is in Drive's trash — filed, but on its way out.
    pub trashed: bool,
    /// Who filed it.
    pub filed_by: UserId,
    /// When.
    pub filed_at: OffsetDateTime,
}

/// The columns of a document read, with the node's own facts joined on.
const DOCUMENT_COLS: &str = "d.id, d.employee_id, d.node_id, d.kind, d.note, n.name AS file_name, \
     n.size, n.content_type, coalesce(n.trashed, false) AS trashed, d.filed_by, d.filed_at";

impl TenantStore {
    /// Files a Drive node against an employee — **the HR door**.
    ///
    /// The node must be in this tenant's HR area and not already filed against
    /// somebody: a contract belongs to the person it names, and one file with
    /// two answers to "whose is this?" is a filing cabinet nobody trusts.
    ///
    /// `actor` is who filed it. A `TenantStore` has no caller of its own, and
    /// "who put this on my file" is a question with standing behind it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the employee is not this tenant's, or the
    /// node is not a live node in this tenant's HR area — one answer for both,
    /// so neither is an existence oracle. [`StoreError::Validation`] when the
    /// note is too long. [`StoreError::Conflict`] when the node is already
    /// filed. [`StoreError::Db`] on failure.
    pub async fn file_hr_document(
        &self,
        employee: &HrEmployeeId,
        node: &DriveNodeId,
        kind: HrDocumentKind,
        note: &str,
        actor: &UserId,
    ) -> Result<HrDocumentId> {
        let note = note.trim();
        if note.chars().count() > DOCUMENT_NOTE_MAX_CHARS {
            return Err(StoreError::Validation(format!(
                "document note must be at most {DOCUMENT_NOTE_MAX_CHARS} characters"
            )));
        }
        self.assert_hr_employee(employee).await?;
        self.assert_hr_area_node(node).await?;
        let id = HrDocumentId::generate();
        sqlx::query(
            "INSERT INTO hr_documents (tenant_id, id, employee_id, node_id, kind, note, filed_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(employee.as_str())
        .bind(node.as_str())
        .bind(kind.as_str())
        .bind(note)
        .bind(actor.as_str())
        .execute(self.pool())
        .await
        .map_err(filing_conflict)?;
        Ok(id)
    }

    /// What is on a person's file, newest first — **the HR door**.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the employee is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_documents(&self, employee: &HrEmployeeId) -> Result<Vec<HrDocument>> {
        self.assert_hr_employee(employee).await?;
        let rows = sqlx::query_as::<_, DocumentRow>(&format!(
            "SELECT {DOCUMENT_COLS} FROM hr_documents d \
               LEFT JOIN drive_nodes n ON n.tenant_id = d.tenant_id AND n.id = d.node_id \
              WHERE d.tenant_id = $1 AND d.employee_id = $2 \
              ORDER BY d.filed_at DESC, d.id"
        ))
        .bind(self.tenant().as_str())
        .bind(employee.as_str())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(DocumentRow::into_document).collect()
    }

    /// One filing, when it is this tenant's and on this employee's file.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the employee is not this tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_document(
        &self,
        employee: &HrEmployeeId,
        id: &HrDocumentId,
    ) -> Result<Option<HrDocument>> {
        self.assert_hr_employee(employee).await?;
        let row = sqlx::query_as::<_, DocumentRow>(&format!(
            "SELECT {DOCUMENT_COLS} FROM hr_documents d \
               LEFT JOIN drive_nodes n ON n.tenant_id = d.tenant_id AND n.id = d.node_id \
              WHERE d.tenant_id = $1 AND d.employee_id = $2 AND d.id = $3"
        ))
        .bind(self.tenant().as_str())
        .bind(employee.as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        row.map(DocumentRow::into_document).transpose()
    }

    /// Detaches a document filed against the wrong person — **the HR door**.
    ///
    /// The *file* is untouched: it stays in the HR area, where it can be filed
    /// against the right person or removed through Drive's own trash. Deleting
    /// somebody's contract because a filing was a mistake would answer a
    /// different request than the one that was made.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the employee or the filing is not this
    /// tenant's; [`StoreError::Db`] on failure.
    pub async fn detach_hr_document(
        &self,
        employee: &HrEmployeeId,
        id: &HrDocumentId,
    ) -> Result<()> {
        self.assert_hr_employee(employee).await?;
        let done = sqlx::query(
            "DELETE FROM hr_documents WHERE tenant_id = $1 AND employee_id = $2 AND id = $3",
        )
        .bind(self.tenant().as_str())
        .bind(employee.as_str())
        .bind(id.as_str())
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Ok when the employee is this tenant's, else [`StoreError::NotFound`] —
    /// the same answer an id that was never issued gets, so a filing route is
    /// not an oracle for another tenant's people.
    async fn assert_hr_employee(&self, employee: &HrEmployeeId) -> Result<()> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM hr_employees WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(self.tenant().as_str())
        .bind(employee.as_str())
        .fetch_one(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if exists {
            Ok(())
        } else {
            Err(StoreError::NotFound)
        }
    }

    /// Ok when the node is a live node in **this tenant's HR area**.
    ///
    /// The tenant, the area and the trash flag are one query and one answer:
    /// another tenant's node, a node in somebody's personal files, a node in a
    /// Space and a node already in the trash are all [`StoreError::NotFound`],
    /// because each of them is equally not a document HR may file.
    async fn assert_hr_area_node(&self, node: &DriveNodeId) -> Result<()> {
        let row: Option<(String, String, bool)> = sqlx::query_as(
            "SELECT location_kind, location_id, trashed FROM drive_nodes \
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant().as_str())
        .bind(node.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some((kind, id, false)) if kind == HR_AREA && id == self.tenant().as_str() => Ok(()),
            _ => Err(StoreError::NotFound),
        }
    }
}

/// Turns the one unique-index violation this table can raise into a conflict
/// that names the rule without naming whose file the node is already on — which
/// would be an oracle for a record the caller was not reading.
fn filing_conflict(error: sqlx::Error) -> StoreError {
    let constraint = match &error {
        sqlx::Error::Database(db) => db.constraint().unwrap_or_default().to_owned(),
        _ => String::new(),
    };
    if constraint == "hr_documents_node_unique" {
        StoreError::Conflict("that file is already filed against an employee".to_owned())
    } else {
        StoreError::from(error)
    }
}

#[derive(sqlx::FromRow)]
struct DocumentRow {
    id: String,
    employee_id: String,
    node_id: String,
    kind: String,
    note: String,
    file_name: Option<String>,
    size: Option<i64>,
    content_type: Option<String>,
    trashed: bool,
    filed_by: String,
    filed_at: OffsetDateTime,
}

impl DocumentRow {
    fn into_document(self) -> Result<HrDocument> {
        Ok(HrDocument {
            id: HrDocumentId::new(self.id),
            employee_id: HrEmployeeId::new(self.employee_id),
            node_id: DriveNodeId::new(self.node_id),
            kind: HrDocumentKind::parse(&self.kind)?,
            note: self.note,
            file_name: self.file_name,
            size: self.size,
            content_type: self.content_type,
            trashed: self.trashed,
            filed_by: UserId::new(self.filed_by),
            filed_at: self.filed_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::HrDocumentKind;

    #[test]
    fn every_kind_round_trips_through_its_stored_word() {
        for kind in [
            HrDocumentKind::Contract,
            HrDocumentKind::Amendment,
            HrDocumentKind::Letter,
            HrDocumentKind::Certificate,
            HrDocumentKind::Other,
        ] {
            let parsed = HrDocumentKind::parse(kind.as_str());
            assert_eq!(parsed.ok(), Some(kind), "{kind} did not round trip");
        }
    }

    #[test]
    fn a_word_no_code_knows_is_refused_with_the_list() {
        let refused = HrDocumentKind::parse("payslip");
        let message = match refused {
            Err(error) => error.to_string(),
            Ok(kind) => panic!("expected a refusal, got {kind}"),
        };
        assert!(message.contains("contract"), "the message lists the set");
    }
}
