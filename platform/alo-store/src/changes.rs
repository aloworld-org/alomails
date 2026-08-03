//! JMAP change tracking (kept out of `store.rs` — Law 3): a **per-account**
//! monotonic `modseq` and a per-object change log that answers
//! `/changes` (created/updated/destroyed since a state) in one indexed
//! range scan.
//!
//! The counter is keyed by `(tenant_id, user_id)`, not just tenant, so a
//! user's state cursor advances only on that user's own mutations — a
//! co-tenant cannot infer another's activity volume from the token
//! (see migration `0005`, `docs/design/account-scoped-access-door.md`).

use sqlx::PgPool;

use crate::error::Result;

/// JMAP object type names tracked for changes.
pub const TYPE_MAILBOX: &str = "Mailbox";
/// See [`TYPE_MAILBOX`].
pub const TYPE_EMAIL: &str = "Email";
/// See [`TYPE_MAILBOX`].
pub const TYPE_THREAD: &str = "Thread";
/// See [`TYPE_MAILBOX`].
pub const TYPE_CONTACT: &str = "Contact";

/// What happened to an object.
#[derive(Debug, Clone, Copy)]
pub enum ChangeKind {
    /// The object was created.
    Created,
    /// The object was modified.
    Updated,
    /// The object was destroyed (a tombstone is kept).
    Destroyed,
}

/// One change to record within a mutating transaction.
pub struct Change<'a> {
    /// Object type (`Mailbox`/`Email`/`Thread`).
    pub obj_type: &'a str,
    /// Object id.
    pub id: &'a str,
    /// What happened.
    pub kind: ChangeKind,
}

impl<'a> Change<'a> {
    /// A created-object change.
    pub fn created(obj_type: &'a str, id: &'a str) -> Self {
        Self {
            obj_type,
            id,
            kind: ChangeKind::Created,
        }
    }
    /// An updated-object change.
    pub fn updated(obj_type: &'a str, id: &'a str) -> Self {
        Self {
            obj_type,
            id,
            kind: ChangeKind::Updated,
        }
    }
    /// A destroyed-object change.
    pub fn destroyed(obj_type: &'a str, id: &'a str) -> Self {
        Self {
            obj_type,
            id,
            kind: ChangeKind::Destroyed,
        }
    }
}

/// Bumps this account's modseq once and records every change at the new
/// modseq, all inside the caller's transaction. Every change in one call
/// belongs to a single account (`user`) — JMAP/IMAP change tracking is
/// per-account. Returns the new modseq (the value future state tokens
/// compare against).
pub async fn bump_and_record(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    user: &str,
    changes: &[Change<'_>],
) -> Result<i64> {
    let modseq = sqlx::query!(
        "INSERT INTO account_modseq (tenant_id, user_id, modseq) VALUES ($1, $2, 1) \
         ON CONFLICT (tenant_id, user_id) DO UPDATE SET modseq = account_modseq.modseq + 1 \
         RETURNING modseq",
        tenant,
        user
    )
    .fetch_one(&mut **tx)
    .await?
    .modseq;

    for change in changes {
        let destroyed = matches!(change.kind, ChangeKind::Destroyed);
        sqlx::query!(
            "INSERT INTO object_changes \
             (tenant_id, user_id, type, id, created_modseq, modseq, destroyed) \
             VALUES ($1, $2, $3, $4, $5, $5, $6) \
             ON CONFLICT (tenant_id, type, id) \
             DO UPDATE SET modseq = $5, destroyed = $6, user_id = $2",
            tenant,
            user,
            change.obj_type,
            change.id,
            modseq,
            destroyed
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(modseq)
}

/// This account's current modseq (0 when it has never mutated).
///
/// # Errors
/// [`crate::StoreError::Db`] on failure.
pub async fn current_state(pool: &PgPool, tenant: &str, user: &str) -> Result<i64> {
    let row = sqlx::query!(
        "SELECT modseq FROM account_modseq WHERE tenant_id = $1 AND user_id = $2",
        tenant,
        user
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.modseq).unwrap_or(0))
}

/// The delta a `/changes` call returns.
#[derive(Debug, Clone)]
pub struct Changes {
    /// The state passed in.
    pub old_state: i64,
    /// The state to resume from.
    pub new_state: i64,
    /// Ids created since `old_state`.
    pub created: Vec<String>,
    /// Ids updated since `old_state`.
    pub updated: Vec<String>,
    /// Ids destroyed since `old_state`.
    pub destroyed: Vec<String>,
    /// Whether more changes remain past `new_state`.
    pub has_more: bool,
}

/// One change-log row.
#[derive(Clone)]
struct Row {
    id: String,
    created_modseq: i64,
    modseq: i64,
    destroyed: bool,
}

/// Computes `/changes` for one account's `obj_type` since `since`,
/// bounded by `max` objects. **Per-account:** only the `user`'s objects
/// are returned. The caller validates the token first (a malformed or
/// out-of-range token is `cannotCalculateChanges`).
///
/// `new_state` never advances past a modseq unless *every* row at that
/// modseq is included — several objects can share one modseq (a single
/// transaction records many), and splitting the group would silently
/// drop the un-returned siblings forever.
///
/// # Errors
/// [`crate::StoreError::Db`] on failure.
pub async fn changes_since(
    pool: &PgPool,
    tenant: &str,
    user: &str,
    obj_type: &str,
    since: i64,
    max: i64,
) -> Result<Changes> {
    // Fetch one extra to detect "more".
    let rows = fetch_rows(pool, tenant, user, obj_type, since, max + 1).await?;

    if (rows.len() as i64) <= max {
        // Caught up: resume from this account's current modseq.
        let new_state = current_state(pool, tenant, user).await?.max(since);
        return Ok(classify(&rows, since, new_state, false));
    }

    let boundary = rows[max as usize - 1].modseq; // last modseq we'd return
    let next = rows[max as usize].modseq; // first modseq we would drop
    if next > boundary {
        // Clean split between modseq groups.
        return Ok(classify(&rows[..max as usize], since, boundary, true));
    }

    // The `boundary` group is split by the limit. Prefer returning up to
    // the highest complete group below it.
    let below: Vec<Row> = rows
        .iter()
        .take(max as usize)
        .filter(|r| r.modseq < boundary)
        .cloned()
        .collect();
    if let Some(last) = below.last() {
        let new_state = last.modseq;
        return Ok(classify(&below, since, new_state, true));
    }

    // The whole window is a single group larger than `max`: return the
    // entire group (so a tiny maxChanges still makes progress), and report
    // more if anything exists above it.
    let full_group = fetch_group(pool, tenant, user, obj_type, boundary).await?;
    let has_more_after = sqlx::query!(
        "SELECT 1 AS one FROM object_changes \
         WHERE tenant_id = $1 AND user_id = $2 AND type = $3 AND modseq > $4 LIMIT 1",
        tenant,
        user,
        obj_type,
        boundary
    )
    .fetch_optional(pool)
    .await?
    .is_some();
    Ok(classify(&full_group, since, boundary, has_more_after))
}

async fn fetch_rows(
    pool: &PgPool,
    tenant: &str,
    user: &str,
    obj_type: &str,
    since: i64,
    limit: i64,
) -> Result<Vec<Row>> {
    let rows = sqlx::query!(
        "SELECT id, created_modseq, modseq, destroyed FROM object_changes \
         WHERE tenant_id = $1 AND user_id = $2 AND type = $3 AND modseq > $4 \
         ORDER BY modseq LIMIT $5",
        tenant,
        user,
        obj_type,
        since,
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Row {
            id: r.id,
            created_modseq: r.created_modseq,
            modseq: r.modseq,
            destroyed: r.destroyed,
        })
        .collect())
}

async fn fetch_group(
    pool: &PgPool,
    tenant: &str,
    user: &str,
    obj_type: &str,
    modseq: i64,
) -> Result<Vec<Row>> {
    let rows = sqlx::query!(
        "SELECT id, created_modseq, modseq, destroyed FROM object_changes \
         WHERE tenant_id = $1 AND user_id = $2 AND type = $3 AND modseq = $4",
        tenant,
        user,
        obj_type,
        modseq
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Row {
            id: r.id,
            created_modseq: r.created_modseq,
            modseq: r.modseq,
            destroyed: r.destroyed,
        })
        .collect())
}

fn classify(rows: &[Row], since: i64, new_state: i64, has_more: bool) -> Changes {
    let (mut created, mut updated, mut destroyed) = (Vec::new(), Vec::new(), Vec::new());
    for row in rows {
        if row.destroyed {
            // Created *and* destroyed within the window is a net no-op
            // (JMAP §5.2) — omit it.
            if row.created_modseq <= since {
                destroyed.push(row.id.clone());
            }
        } else if row.created_modseq > since {
            created.push(row.id.clone());
        } else {
            updated.push(row.id.clone());
        }
    }
    Changes {
        old_state: since,
        new_state,
        created,
        updated,
        destroyed,
        has_more,
    }
}
