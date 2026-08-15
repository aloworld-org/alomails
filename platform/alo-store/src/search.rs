//! Workspace search (ADR 0029) — one query across the modules, scoped to exactly
//! what the caller may already see (their personal items, the Spaces they belong
//! to, their visible task projects, their own mailbox). Files and tasks match by
//! name/title; mail matches by full content — the message body is in the mail
//! full-text index, so this searches *inside* the email, not just its subject.
//! Access is applied in SQL, never widened — the same predicates the modules use.
//! Drive files now match on content too: a text file or alo Doc is indexed from
//! its bytes at write time (see `drive_index_node`), so a term inside the
//! document is found. Still to come: text extraction for binary formats
//! (docx/xlsx/pdf) and cross-module relevance ranking.

use crate::account::AccountStore;
use crate::error::{Result, StoreError};

/// One search result, enough to render a row and open the item.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// `folder` | `file` | `doc` | `base` (a Drive node kind), `task`, or
    /// `message` (a mail message).
    pub kind: String,
    pub id: String,
    pub title: String,
    /// Where it lives — a Space id for a Space file, else `None` (personal /
    /// task). Lets the UI open it in the right place.
    pub space: Option<String>,
}

impl AccountStore {
    /// Searches the workspace. Returns up to `limit` Drive nodes (in the caller's
    /// personal files or member Spaces) whose **name or indexed content** matches,
    /// up to `limit` visible active tasks whose title matches — a substring,
    /// case-insensitive — plus up to `limit` of the caller's own messages whose
    /// subject, participants, or **body** match, via the mail full-text index.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn workspace_search(&self, query: &str, limit: i64) -> Result<Vec<SearchHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let mut hits = Vec::new();

        // Drive nodes: name OR content match, in a location the caller can read.
        // `content` is the text index built at write time (plain-text files + alo
        // Docs); it is NULL for un-indexed/binary nodes, which then match on name
        // only.
        let drive = sqlx::query_as::<_, (String, String, String, Option<String>)>(
            "SELECT id, kind, name, \
                    CASE WHEN location_kind = 'space' THEN location_id ELSE NULL END AS space \
             FROM drive_nodes \
             WHERE tenant_id = $1 AND trashed = false \
               AND ( strpos(lower(name), lower($3)) > 0 \
                  OR content @@ plainto_tsquery('simple', $3) ) \
               AND ( (location_kind = 'personal' AND location_id = $2) \
                  OR (location_kind = 'space' AND location_id IN ( \
                        SELECT space_id FROM space_members \
                        WHERE tenant_id = $1 AND user_id = $2)) ) \
             ORDER BY updated_at DESC LIMIT $4",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(q)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        for (id, kind, name, space) in drive {
            hits.push(SearchHit {
                kind,
                id,
                title: name,
                space,
            });
        }

        // Tasks: title match, on a project visible to the caller (team, or their
        // own personal) — the same predicate the task module uses.
        let tasks = sqlx::query_as::<_, (String, String)>(
            "SELECT t.id, t.title FROM tasks t \
             WHERE t.tenant_id = $1 AND t.state = 'active' \
               AND strpos(lower(t.title), lower($3)) > 0 \
               AND t.project_id IN ( \
                     SELECT p.id FROM task_projects p WHERE p.tenant_id = $1 \
                       AND p.archived = false \
                       AND (p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = $2))) \
             ORDER BY t.updated_at DESC LIMIT $4",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(q)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        for (id, title) in tasks {
            hits.push(SearchHit {
                kind: "task".to_owned(),
                id,
                title,
                space: None,
            });
        }

        // (See `workspace_search_terms` for the keyword-aware variant the AI
        // "ask your workspace" flow uses on natural-language questions.)

        // Mail: full-text over the message's subject, participants, AND body —
        // the `search` tsvector the mail module builds and queries. Scoped to the
        // caller's own mail (`user_id`), exactly as `AccountStore::search` is.
        // This is the content-search half: a term only in the body still matches.
        let mail = sqlx::query_as::<_, (String, String)>(
            "SELECT id, subject FROM messages \
             WHERE tenant_id = $1 AND user_id = $2 \
               AND search @@ plainto_tsquery('simple', $3) \
             ORDER BY received_at DESC LIMIT $4",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(q)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        for (id, subject) in mail {
            let title = if subject.trim().is_empty() {
                "(no subject)".to_owned()
            } else {
                subject
            };
            hits.push(SearchHit {
                kind: "message".to_owned(),
                id,
                title,
                space: None,
            });
        }

        Ok(hits)
    }

    /// Keyword-aware retrieval for the AI "ask your workspace" flow (ADR 0029):
    /// the caller passes a whole natural-language question ("what did we decide
    /// about the Acme pricing?"), which we reduce to its content words before
    /// matching — so a file or task matches on *any* keyword, not on the literal
    /// question string. Access scoping is identical to [`Self::workspace_search`]
    /// (files in the caller's personal/member locations, their visible tasks,
    /// their own mail); the AI is never shown more than the caller could open.
    /// Falls back to a plain search when the question has no usable keywords.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn workspace_search_terms(
        &self,
        question: &str,
        limit: i64,
    ) -> Result<Vec<SearchHit>> {
        let terms = keywords(question);
        if terms.is_empty() {
            return self.workspace_search(question, limit).await;
        }
        let mut hits = self.drive_term_hits(&terms, limit).await?;
        hits.extend(self.task_term_hits(&terms, limit).await?);
        hits.extend(self.mail_term_hits(&terms, limit).await?);
        Ok(hits)
    }

    /// Drive nodes matching any keyword by name, or the whole phrase by indexed
    /// content, in a location the caller can read.
    ///
    /// One of the three sources [`Self::workspace_search_terms`] unions, split
    /// out so per-product grounding ([`crate::agent_ground`]) can draw on
    /// exactly one of them without a second copy of the access predicate.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn drive_term_hits(
        &self,
        terms: &[String],
        limit: i64,
    ) -> Result<Vec<SearchHit>> {
        let joined = terms.join(" ");
        let mut hits = Vec::new();
        let drive = sqlx::query_as::<_, (String, String, String, Option<String>)>(
            "SELECT id, kind, name, \
                    CASE WHEN location_kind = 'space' THEN location_id ELSE NULL END AS space \
             FROM drive_nodes \
             WHERE tenant_id = $1 AND trashed = false \
               AND ( EXISTS (SELECT 1 FROM unnest($4::text[]) kw WHERE strpos(lower(name), kw) > 0) \
                  OR content @@ plainto_tsquery('simple', $5) ) \
               AND ( (location_kind = 'personal' AND location_id = $2) \
                  OR (location_kind = 'space' AND location_id IN ( \
                        SELECT space_id FROM space_members \
                        WHERE tenant_id = $1 AND user_id = $2)) ) \
             ORDER BY updated_at DESC LIMIT $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(limit)
        .bind(terms)
        .bind(&joined)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        for (id, kind, name, space) in drive {
            hits.push(SearchHit {
                kind,
                id,
                title: name,
                space,
            });
        }
        Ok(hits)
    }

    /// Active tasks matching any keyword by title, on a project the caller can
    /// see — the same predicate the task module itself uses.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn task_term_hits(
        &self,
        terms: &[String],
        limit: i64,
    ) -> Result<Vec<SearchHit>> {
        let mut hits = Vec::new();
        let tasks = sqlx::query_as::<_, (String, String)>(
            "SELECT t.id, t.title FROM tasks t \
             WHERE t.tenant_id = $1 AND t.state = 'active' \
               AND EXISTS (SELECT 1 FROM unnest($4::text[]) kw WHERE strpos(lower(t.title), kw) > 0) \
               AND t.project_id IN ( \
                     SELECT p.id FROM task_projects p WHERE p.tenant_id = $1 \
                       AND p.archived = false \
                       AND (p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = $2))) \
             ORDER BY t.updated_at DESC LIMIT $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(limit)
        .bind(terms)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        for (id, title) in tasks {
            hits.push(SearchHit {
                kind: "task".to_owned(),
                id,
                title,
                space: None,
            });
        }
        Ok(hits)
    }

    /// The caller's **own** messages matching ANY keyword against the mail
    /// full-text index (subject, participants and body).
    ///
    /// ORing per-term matters because a request is often action-phrased
    /// ("archive the Acme newsletter") — the verb is a keyword but never appears
    /// in the email, so ANDing every term would exclude the very message being
    /// referenced.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn mail_term_hits(
        &self,
        terms: &[String],
        limit: i64,
    ) -> Result<Vec<SearchHit>> {
        let mut hits = Vec::new();
        let mail = sqlx::query_as::<_, (String, String)>(
            "SELECT id, subject FROM messages \
             WHERE tenant_id = $1 AND user_id = $2 \
               AND EXISTS (SELECT 1 FROM unnest($3::text[]) kw \
                             WHERE search @@ plainto_tsquery('simple', kw)) \
             ORDER BY received_at DESC LIMIT $4",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(terms)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        for (id, subject) in mail {
            let title = if subject.trim().is_empty() {
                "(no subject)".to_owned()
            } else {
                subject
            };
            hits.push(SearchHit {
                kind: "message".to_owned(),
                id,
                title,
                space: None,
            });
        }

        Ok(hits)
    }
}

/// Reduces a natural-language question to its content keywords: lowercase words
/// of three or more characters, minus a small stop-word list, de-duplicated,
/// capped. Empty when the question is all stop-words/punctuation (the caller
/// then falls back to a literal search).
pub(crate) fn keywords(question: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "you", "your", "this", "that", "with", "from", "about", "have", "has",
        "had", "what", "which", "where", "who", "whom", "when", "why", "how", "are", "was", "were",
        "did", "does", "can", "could", "would", "should", "will", "into", "our", "their", "they",
        "them", "there", "here", "any", "all", "some", "get", "got", "give", "tell", "show",
        "find", "was", "not", "but", "out", "off",
    ];
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for word in question.split(|c: char| !c.is_alphanumeric()) {
        let word = word.to_lowercase();
        if word.len() >= 3 && !STOP.contains(&word.as_str()) && seen.insert(word.clone()) {
            out.push(word);
            if out.len() >= 12 {
                break;
            }
        }
    }
    out
}
