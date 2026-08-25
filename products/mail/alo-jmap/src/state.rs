//! Shared service state, honest limits, and bearer authentication.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use alo_identity::Identity;
use alo_store::{AccountStore, AppModule, Page, Store, TenantId, TenantRole, UserId};
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;

use crate::error::Problem;
use crate::push::PushHub;

/// Process-wide JMAP service state.
#[derive(Clone)]
pub struct AppState {
    /// The message store (system handle).
    pub store: Arc<Store>,
    /// The credential authority — resolves bearer tokens to accounts.
    pub identity: Identity,
    /// Per-tenant push fan-out for EventSource.
    pub push: PushHub,
    /// Agent turns running on this process right now (ADR 0034's Stop
    /// control). In memory on purpose: a turn in flight is a fact about this
    /// process for a few seconds, not about the workspace.
    pub turns: crate::chat_turns::Turns,
    /// The media engine behind alo Meet, when this deployment has one.
    /// `None` means meetings can be recorded but not held — which is a
    /// deployment fact the join route reports plainly rather than a failure.
    pub media: Option<MediaEngine>,
    /// Advertised, enforced limits.
    pub limits: Limits,
    /// Externally-visible base URL, for building session URLs.
    pub base_url: String,
    /// `host:port` of the SMTP trusted internal submission listener, used by
    /// `EmailSubmission/set` to send. `None` disables sending: the capability
    /// is still advertised, and a submit answers `serverFail` — a deployment
    /// that cannot send at all is our fault to fix, not the caller's.
    pub submission_addr: Option<String>,
    /// Extra hosts this deployment serves the API on, besides the configured
    /// `base_url`.
    ///
    /// One service answers several front-ends — the mail app and the workspace
    /// app — and the JMAP Session resource must advertise its URLs on whichever
    /// origin the client actually reached, or the client's own `connect-src`
    /// blocks every call it then makes. An allowlist rather than trusting the
    /// `Host` header: that header is caller-controlled, and echoing it into a
    /// URL the client will call is the shape of an open redirect.
    ///
    /// From `ALO_JMAP_SESSION_ORIGINS`, comma-separated hosts (no scheme).
    pub session_origins: Vec<String>,

    /// Whether the MAPI-over-HTTP adapter is served here (ADR 0051), from
    /// `ALO_MAPI_HTTP_ENABLED`. **Off by default, deliberately:** while it is
    /// off Autodiscover stays silent about `mapiHttp`, because an Outlook told
    /// to speak a protocol we do not answer does not fall back to the IMAP
    /// settings sitting in the same document — it fails to configure at all.
    /// Turning this on is the rollout; turning it off is the rollback.
    pub mapi_http: bool,

    /// Live MAPI Session Contexts. Held here rather than inside the MAPI router
    /// so the contexts outlive any one router build and can later be counted by
    /// a metric alongside everything else this state owns.
    pub mapi_sessions: std::sync::Arc<alo_mapi::SessionStore>,

    /// Junk training: Rspamd learn calls on moves into/out of Junk.
    /// `None` disables training (mail management is unaffected).
    pub junk_learner: Option<std::sync::Arc<crate::junk_learn::JunkLearner>>,
    /// Domains open to self-service personal signup (ADR 0018), lowercased.
    /// Empty disables the signup surface.
    pub personal_domains: Vec<String>,
    /// In-process throttle for the public signup endpoints.
    pub signup_limiter: alo_identity::ratelimit::RateLimiter,
}

/// The limits advertised in the Session resource and enforced on every
/// request. Real values, documented in `docs/design/jmap-api.md`.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// `maxSizeUpload` (octets).
    pub max_size_upload: u64,
    /// `maxSizeRequestObject` (octets) — bounded before parse.
    pub max_size_request: usize,
    /// `maxConcurrentUpload`.
    pub max_concurrent_upload: u64,
    /// `maxCallsInRequest`.
    pub max_calls_in_request: usize,
    /// `maxObjectsInGet`.
    pub max_objects_in_get: usize,
    /// `maxObjectsInSet`.
    pub max_objects_in_set: usize,
    /// Truncation ceiling for `Email/get` `bodyValues`.
    pub max_body_value_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_size_upload: 50 * 1024 * 1024,
            max_size_request: 10 * 1024 * 1024,
            max_concurrent_upload: 4,
            max_calls_in_request: 32,
            max_objects_in_get: 500,
            max_objects_in_set: 500,
            max_body_value_bytes: 256 * 1024,
        }
    }
}

/// An authenticated account: the resolved tenant/user and the store door
/// scoped to that `(tenant, user)`. Obtained only via [`authenticate`].
/// The door bakes both ids, so every store call is account-scoped by
/// construction — no ownership guard to remember.
///
/// `Clone` so a chat agent's turn can run off the request that triggered it and
/// still act through the asker's own door. Cloning copies the claims already
/// resolved; it can never widen them, because there is no constructor here that
/// takes a tenant or a user — only [`authenticate`] makes one.
#[derive(Clone)]
pub struct Account {
    /// The tenant claim (from the token, never the request body).
    pub tenant: TenantId,
    /// The account's user.
    pub user: UserId,
    /// The account-scoped store handle — the only path to this user's
    /// mail data.
    pub acc: AccountStore,
    /// Whether this user is a tenant admin (gates admin-only surfaces).
    pub is_admin: bool,
    /// The tenant-wide scoped roles this user holds (ADR 0035, B4.12) — today
    /// scoped roles such as [`TenantRole::Accountant`]. A role is never an admin flag: it opens
    /// the surfaces its own gates name and nothing else, and a delegated handle
    /// carries none for the same reason it carries no admin.
    pub roles: Vec<TenantRole>,
    /// The rail modules a tenant admin has switched off for this person
    /// (migration 0208). Ordinarily empty.
    ///
    /// This only ever **narrows**. An app that is not denied still needs
    /// whatever its own gate wants — Finance an accountant, a Space its
    /// membership — and an admin is never denied, because the switch lives in
    /// the console an admin would need to reach to undo it.
    pub denied_modules: Vec<AppModule>,
    /// Delegation status of THIS account handle (ADR 0017). `None` when it is
    /// the signed-in user's own account (full rights). `Some(..)` when it is
    /// another user's mailbox the signed-in user was granted access to — the
    /// grant carries the access level and send mode, and the acting delegate's
    /// id (for the on-behalf `Sender:`). A delegated handle never confers admin.
    pub delegated: Option<Delegation>,
}

/// How a delegate may send from a shared mailbox (ADR 0017).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SendMode {
    /// No sending.
    None,
    /// Send *as* the owner — `From:` the shared address, no `Sender:`.
    As,
    /// Send *on behalf of* the owner — `From:` the shared address plus a
    /// `Sender:` of the acting delegate (recipients see who actually sent).
    OnBehalf,
}

impl SendMode {
    /// Parse the stored `send_mode` value.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "as" => Self::As,
            "on_behalf" => Self::OnBehalf,
            _ => Self::None,
        }
    }

    /// Whether sending is permitted at all.
    #[must_use]
    pub fn can_send(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// A resolved delegation grant carried on a delegated [`Account`] handle.
#[derive(Clone)]
pub struct Delegation {
    /// Whether the delegate may manage the mailbox (move/flag/delete), else
    /// read-only.
    pub can_write: bool,
    /// How the delegate may send from the mailbox.
    pub send_mode: SendMode,
    /// The acting delegate (the signed-in user) — the on-behalf `Sender:`.
    pub delegate: UserId,
    /// Per-folder restriction (ADR 0017): `None` = whole mailbox; `Some(set)` =
    /// only these folders are visible/touchable, every other folder invisible.
    pub folders: Option<std::collections::HashSet<String>>,
}

impl Delegation {
    /// Whether the delegate may touch folder `mailbox_id`. Always true when
    /// unrestricted (whole-mailbox); otherwise only the granted folders.
    pub fn folder_allowed(&self, mailbox_id: &str) -> bool {
        match &self.folders {
            None => true,
            Some(set) => set.contains(mailbox_id),
        }
    }

    /// Whether at least one of a message's folders is accessible — a message is
    /// visible to a restricted delegate iff it lives in a granted folder.
    pub fn any_folder_allowed<I, S>(&self, mailbox_ids: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        match &self.folders {
            None => true,
            Some(set) => mailbox_ids.into_iter().any(|m| set.contains(m.as_ref())),
        }
    }
}

impl Account {
    /// The JMAP accountId (the user id).
    pub fn account_id(&self) -> &str {
        self.user.as_str()
    }

    /// Guard for admin-only endpoints.
    ///
    /// # Errors
    /// [`Problem`] 403 when the user is not a tenant admin.
    pub fn require_admin(&self) -> Result<(), Problem> {
        if self.is_admin {
            Ok(())
        } else {
            Err(Problem::with(
                axum::http::StatusCode::FORBIDDEN,
                "admin only",
            ))
        }
    }

    /// Whether this account holds a tenant-wide scoped role.
    pub fn has_role(&self, role: TenantRole) -> bool {
        self.roles.contains(&role)
    }

    /// Whether a tenant admin has left this app switched on for this person
    /// (migration 0208).
    ///
    /// Answers only that one question. A `true` is not permission to use the
    /// module — every gate the module already had still applies — it says the
    /// per-user switch is not shut. An admin always passes.
    pub fn may_open(&self, module: AppModule) -> bool {
        self.is_admin || !self.denied_modules.contains(&module)
    }

    /// Guard for the privileged finance surfaces — the reports, the approvals
    /// inbox and the period lock (ADR 0035, B4.12).
    ///
    /// Widens [`Account::require_admin`] by exactly one role: an **accountant**
    /// passes it too. It is the whole point of the role — the books belong to
    /// the person who keeps them, and until now keeping them meant holding the
    /// admin console, the mail and the files as well.
    ///
    /// # Errors
    /// [`Problem`] 403 when the user is neither a tenant admin nor an
    /// accountant.
    pub fn require_finance(&self) -> Result<(), Problem> {
        if self.is_admin || self.has_role(TenantRole::Accountant) {
            Ok(())
        } else {
            Err(Problem::with(
                axum::http::StatusCode::FORBIDDEN,
                "admin or accountant only",
            ))
        }
    }

    /// Guard for the HR door — the whole employee record including the private
    /// fields, the terms and pay, and the papers on somebody's file (alo HR,
    /// ADR 0035, wave B6.02b; `docs/design/hr.md`, "The HR role").
    ///
    /// Widens [`Account::require_admin`] by exactly one role, and deliberately
    /// **not** by the accountant's: an external bookkeeper reading everybody's
    /// contract and home address is precisely the failure the scoped roles
    /// exist to prevent, so somebody who genuinely runs both is granted both.
    ///
    /// It gates the *record*, never the directory: the people list and the org
    /// chart are every member's read, and narrowing them to HR would put the
    /// company's org chart back in a filing cabinet.
    ///
    /// # Errors
    /// [`Problem`] 403 naming the role when the caller is neither a tenant
    /// admin nor an HR user.
    pub fn require_hr(&self) -> Result<(), Problem> {
        if self.is_admin || self.has_role(TenantRole::Hr) {
            Ok(())
        } else {
            Err(Problem::with(
                axum::http::StatusCode::FORBIDDEN,
                "admin or hr only",
            ))
        }
    }
}

/// Resolves the `Authorization: Bearer` token to an [`Account`] via
/// `alo-identity`. The tenant is taken from the token, never the
/// request. A revoked or expired token resolves to `unauthorized`.
///
/// # Errors
/// [`Problem::unauthorized`] when the token is missing/invalid/revoked;
/// [`Problem::server_error`] on a store failure.
pub async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<Account, Problem> {
    let token = bearer_token(headers).ok_or_else(Problem::unauthorized)?;
    let principal = state
        .identity
        .resolve_access_token(&token)
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(Problem::unauthorized)?;
    let acc = state
        .store
        .for_account(principal.tenant.clone(), principal.user.clone());
    // The admin flag and the scoped roles in ONE read: `authenticate` runs on
    // every request in the product, and a second round trip to learn a fact
    // almost nobody has would be paid by the mail hot path forever. A store
    // failure is read as no access rather than as an error, exactly as the
    // admin flag alone was — a request that cannot learn its caller's rights
    // proceeds with none of them.
    let facts = acc.access_facts().await.unwrap_or_default();
    Ok(Account {
        tenant: principal.tenant,
        user: principal.user,
        acc,
        is_admin: facts.is_admin,
        roles: facts.roles,
        denied_modules: facts.denied_modules,
        delegated: None,
    })
}

/// Resolves the account a request targets (its `accountId`) into an [`Account`]
/// handle the signed-in user is authorized to operate on, or `None` when they
/// are not — which the caller renders as the same `accountNotFound` as any
/// unknown id (no oracle for "exists but you can't touch it").
///
/// - the signed-in user's own id → their own account (full rights);
/// - another user's id they hold a delegation grant on (same tenant) → that
///   user's mailbox as a delegated handle (`is_admin` forced false, and no
///   scoped roles);
/// - anything else → `None`.
pub async fn resolve_target(
    signed_in: &Account,
    state: &AppState,
    account_id: &str,
) -> Option<Account> {
    if account_id == signed_in.user.as_str() {
        return Some(Account {
            tenant: signed_in.tenant.clone(),
            user: signed_in.user.clone(),
            acc: state
                .store
                .for_account(signed_in.tenant.clone(), signed_in.user.clone()),
            is_admin: signed_in.is_admin,
            roles: signed_in.roles.clone(),
            denied_modules: signed_in.denied_modules.clone(),
            delegated: None,
        });
    }
    // A mailbox the signed-in user was delegated access to. The grant is looked
    // up only within the signed-in user's own tenant, so it can never authorize
    // across tenants.
    let owner = UserId::new(account_id);
    let tenant_store = state.store.for_tenant(signed_in.tenant.clone());
    let (can_write, send_mode) = tenant_store
        .delegation(&owner, &signed_in.user)
        .await
        .ok()
        .flatten()?;
    let acc = state
        .store
        .for_account(signed_in.tenant.clone(), owner.clone());
    // Per-folder restriction (ADR 0017): an empty grant means whole-mailbox.
    // A granted folder implicitly includes its subfolders, so expand the raw
    // grant to its descendant closure before enforcement.
    let granted = tenant_store
        .delegate_folders(&owner, &signed_in.user)
        .await
        .unwrap_or_default();
    let folders = if granted.is_empty() {
        None
    } else {
        Some(expand_folder_grant(&acc, granted).await)
    };
    Some(Account {
        tenant: signed_in.tenant.clone(),
        acc,
        user: owner,
        is_admin: false,
        // A delegated handle confers neither the admin flag nor a scoped role:
        // the grant is about one mailbox, and the roles belong to the person
        // who was signed in, not to the mailbox they were let into.
        roles: Vec::new(),
        // A delegated handle carries the *delegate's* app switches, not the
        // mailbox owner's: the person acting is the one being restricted, and
        // reading the owner's would let somebody borrow an app they were
        // denied by being let into a mailbox.
        denied_modules: signed_in.denied_modules.clone(),
        delegated: Some(Delegation {
            can_write,
            send_mode: SendMode::parse(&send_mode),
            delegate: signed_in.user.clone(),
            folders,
        }),
    })
}

/// Expands a per-folder grant to include every descendant of each granted
/// folder — granting a folder implicitly grants its subfolders (ADR 0017). On a
/// store error the raw grant is used unchanged (fails closed to fewer folders).
async fn expand_folder_grant(acc: &AccountStore, granted: Vec<String>) -> HashSet<String> {
    let mut allowed: HashSet<String> = granted.into_iter().collect();
    let boxes = match acc.mailboxes(Page::first(alo_store::MAX_PAGE)).await {
        Ok(b) => b,
        Err(_) => return allowed,
    };
    // parent id → child ids
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for m in &boxes {
        if let Some(parent) = m.parent_id.as_ref() {
            children
                .entry(parent.as_str().to_owned())
                .or_default()
                .push(m.id.as_str().to_owned());
        }
    }
    let mut stack: Vec<String> = allowed.iter().cloned().collect();
    while let Some(id) = stack.pop() {
        if let Some(kids) = children.get(&id) {
            for kid in kids {
                if allowed.insert(kid.clone()) {
                    stack.push(kid.clone());
                }
            }
        }
    }
    allowed
}

/// The raw bearer token from an `Authorization` header, if there is one.
/// `pub(crate)` for the audit layer (B2.13), which resolves the actor of a
/// mutation it did not itself authenticate.
pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

/// How to reach the media engine, and how to sign for it.
///
/// The secret stays here and is never sent anywhere: a browser receives a
/// minted token, never a key, and so cannot mint another.
#[derive(Clone)]
pub struct MediaEngine {
    /// The URL a browser connects to, e.g. `wss://meet.example.com`.
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
}

impl MediaEngine {
    /// Read it from the environment, or `None` when it is not configured.
    ///
    /// All three parts or nothing: a half-configured engine produces tokens
    /// that are refused, which is harder to diagnose than an absent one.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("ALO_MEET_URL").ok()?;
        let api_key = std::env::var("ALO_MEET_API_KEY").ok()?;
        let api_secret = std::env::var("ALO_MEET_API_SECRET").ok()?;
        if url.trim().is_empty() || api_key.trim().is_empty() || api_secret.trim().is_empty() {
            return None;
        }
        Some(Self {
            url,
            api_key,
            api_secret,
        })
    }
}
