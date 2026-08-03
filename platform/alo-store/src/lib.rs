//! # alo-store — the tenant-scoped message store
//!
//! Owns: mailboxes, messages, flags, threads, and blob metadata on
//! PostgreSQL, with message bytes in Garage (S3). This is where customer
//! data comes to rest and where **tenancy is structural**: mail data is
//! reachable only through a [`TenantStore`], obtained solely via
//! [`Store::for_tenant`], and every query it issues carries its tenant
//! predicate by construction (see `docs/design/message-store.md`).

pub mod account;
pub mod account_imap;
pub mod account_sieve;
pub mod audit;
pub mod blob;
pub mod calendar;
pub mod changes;
pub mod contacts;
pub mod control;
pub mod dkim;
pub mod dmarc_reports;
pub mod document;
pub mod error;
pub mod ical;
pub mod id;
pub mod identity;
pub mod maintenance;
pub mod message;
pub mod model;
pub mod rfc2047;
pub mod schedule;
pub mod settings;
pub mod share;
pub mod reset;
pub mod signup;
pub mod snooze;
pub mod store;
pub mod thread;
pub mod vcard;

pub use account::AccountStore;
pub use account_imap::{ImapEntry, ImapMailbox, ImapSearchRow};
pub use account_sieve::{OutboundAction, SieveDelivery, SieveScriptMeta};
#[cfg(feature = "garage")]
pub use blob::GarageConfig;
pub use blob::{BlobStore, ShareStream};
pub use changes::Changes;
pub use contacts::AddressHeaders;
pub use control::PLATFORM_TENANT_NAME;
pub use dkim::DkimSigningMaterial;
pub use dmarc_reports::{DmarcAggregateRow, DmarcEventRecord};
pub use document::{Document, DocumentSummary};
pub use error::{Result, StoreError};
pub use id::{
    BlobId, CategoryId, ContactId, EventId, GroupId, MailboxId, MessageId, TenantId, ThreadId,
    UserId,
};
pub use identity::{
    AccessTokenRow, AuthCodeOutcome, AuthCodeRow, CredentialRow, OAuthClient, PublicKeyRow,
    RefreshTokenRow, SigningKeyRow, TotpRow,
};
pub use model::{
    AiConfigRow, AiProviderRow, AuditEntry, Blob, CalendarEvent, Category, Contact, ContactField,
    DkimKeyRow,
    DomainRow, EmailFilter, EmailQuery, GroupRow, MAX_PAGE, Mailbox, Message, MessageSummary, Page,
    SortDirection, TenantSummary, UserRow,
};
pub use schedule::DueSend;
pub use share::{ShareCreated, ShareTarget};
pub use reset::PendingReset;
pub use signup::PendingSignup;
pub use store::{CATEGORY_KEYWORD_PREFIX, SEEN, Store, TenantStore, category_keyword};
