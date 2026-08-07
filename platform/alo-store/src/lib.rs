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
pub mod base;
pub mod billing_bills;
pub mod billing_cadence;
pub mod billing_cii_read;
pub mod billing_customers;
pub mod billing_einvoice_import;
pub mod billing_field;
pub mod billing_fx;
pub mod billing_fx_ecb;
pub mod billing_fx_rates;
pub mod billing_invoices;
pub mod billing_line;
pub mod billing_payments;
pub mod billing_products;
pub mod billing_quotes;
pub mod billing_schedules;
pub mod billing_sepa;
pub mod billing_sequence;
pub mod billing_settings;
pub mod billing_totals;
pub mod billing_ubl_read;
pub mod billing_vat_report;
pub mod billing_xml_tree;
pub mod blob;
pub mod calendar;
pub mod changes;
pub mod contacts;
pub mod control;
pub mod crm_activities;
pub mod crm_deal_threads;
pub mod crm_deals;
pub mod crm_handoff;
pub mod crm_lead_import;
pub mod crm_next_steps;
pub mod crm_pipelines;
pub mod crm_report;
pub mod crm_stages;
pub mod crm_thread_match;
pub mod csv_read;
pub mod dkim;
pub mod dmarc_reports;
pub mod document;
pub mod drive;
pub mod error;
pub mod extract;
pub mod iban;
pub mod ical;
pub mod id;
pub mod identity;
pub mod maintenance;
pub mod message;
pub mod model;
pub mod reset;
pub mod rfc2047;
pub mod schedule;
pub mod search;
pub mod settings;
pub mod share;
pub mod signup;
pub mod site_assets;
pub mod site_model;
pub mod site_pages;
pub mod site_public;
pub mod site_publish;
pub mod site_theme;
pub mod sites;
pub mod snooze;
pub mod spaces;
pub mod store;
pub mod tasks;
pub mod thread;
pub mod vat_id;
pub mod vcard;

pub use account::AccountStore;
pub use account_imap::{ImapEntry, ImapMailbox, ImapSearchRow};
pub use account_sieve::{OutboundAction, SieveDelivery, SieveScriptMeta};
pub use base::{Base, BaseField, BaseRecord, BaseTable, BaseView};
pub use billing_bills::{Bill, BillDocument, BillStatus, BillTotals, NewBill, Supplier};
pub use billing_cadence::{Cadence, next_occurrence};
pub use billing_customers::{Customer, NewCustomer};
pub use billing_einvoice_import::{EInvoiceSyntax, InboundInvoice, parse_einvoice};
pub use billing_fx::FxSnapshot;
pub use billing_fx_rates::{FxImport, FxRate, FxRateSource};
pub use billing_invoices::{Invoice, InvoiceDocument, InvoiceStatus, InvoiceSummary, NewInvoice};
pub use billing_line::{Line, NewLine};
pub use billing_payments::{NewPayment, Payment, PaymentState, Settlement};
pub use billing_products::{NewProduct, Product};
pub use billing_quotes::{
    NewQuote, Quote, QuoteAcceptance, QuoteDocument, QuoteStatus, QuoteSummary,
};
pub use billing_schedules::{
    NewSchedule, Schedule, ScheduleDocument, ScheduleEdit, ScheduleRun, ScheduleSummary,
};
pub use billing_sepa::{CreditTransfer, PaymentFile};
pub use billing_sequence::{
    INVOICE_NUMBER_PREFIX, INVOICE_SEQUENCE_KIND, QUOTE_NUMBER_PREFIX, QUOTE_SEQUENCE_KIND,
    document_number,
};
pub use billing_settings::{BillingSettings, NewBillingSettings};
pub use billing_totals::{LineFigures, Totals, VatSubtotal};
pub use billing_vat_report::{VatPeriod, VatPeriodBase, VatPeriodCurrency, VatPeriodRate};
#[cfg(feature = "garage")]
pub use blob::GarageConfig;
pub use blob::{BlobStore, ShareStream};
pub use changes::Changes;
pub use contacts::AddressHeaders;
pub use control::PLATFORM_TENANT_NAME;
pub use crm_activities::{Activity, ActivityKind, NewActivity};
pub use crm_deal_threads::{DealThread, ThreadSuggestion};
pub use crm_deals::{Deal, DealFilter, DealState, NewDeal, StageEvent, StageMove};
pub use crm_handoff::DealHandoff;
pub use crm_lead_import::{
    DuplicateReason, DuplicateRow, DuplicateSource, LeadImportReport, LeadImportRequest,
    LeadMapping, LeadRow, RowError,
};
pub use crm_next_steps::DEAL_SOURCE_KIND;
pub use crm_pipelines::{NewPipeline, Pipeline, PipelineSeed, StageSeed};
pub use crm_report::{PipelineCurrency, PipelineReport, PipelineStageRow, PipelineTally};
pub use crm_stages::{NewStage, Stage};
pub use crm_thread_match::MatchReason;
pub use csv_read::{CsvEncoding, CsvRow, CsvTable};
pub use dkim::DkimSigningMaterial;
pub use dmarc_reports::{DmarcAggregateRow, DmarcEventRecord};
pub use document::{Document, DocumentSummary};
pub use drive::{DriveLocation, DriveNode, DriveVersion, NewDriveFile};
pub use error::{Result, StoreError};
pub use id::{
    AttachmentId, BaseFieldId, BaseRecordId, BaseTableId, BaseViewId, BillingBillId,
    BillingCustomerId, BillingInvoiceId, BillingLineId, BillingPaymentId, BillingProductId,
    BillingQuoteId, BillingScheduleId, BlobId, CalendarId, CategoryId, CommentId, ContactId,
    CrmActivityId, CrmDealId, CrmEventId, CrmPipelineId, CrmStageId, DriveNodeId, EventId, GroupId,
    LabelId, MailboxId, MessageId, ProjectId, SiteId, SitePageId, SitePublishId, SpaceId,
    SubtaskId, TaskId, TenantId, ThreadId, UserId,
};
pub use identity::{
    AccessTokenRow, AuthCodeOutcome, AuthCodeRow, CredentialRow, OAuthClient, PublicKeyRow,
    RefreshTokenRow, SigningKeyRow, TotpRow,
};
pub use model::{
    AiConfigRow, AiProviderRow, AuditEntry, Blob, Calendar, CalendarEvent, CalendarGrant, Category,
    Contact, ContactField, DkimKeyRow, DomainRow, EmailFilter, EmailQuery, GroupRow, MAX_PAGE,
    Mailbox, Message, MessageSummary, OccurrenceOverride, Page, SortDirection, TenantSummary,
    UserRow,
};
pub use reset::PendingReset;
pub use schedule::DueSend;
pub use search::SearchHit;
pub use share::{ShareCreated, ShareTarget};
pub use signup::PendingSignup;
pub use site_assets::{SITE_IMAGE_CONTENT_TYPES, SiteImageData, site_image_content_type};
pub use site_model::{SECTIONS_SCHEMA_VERSION, Section, SectionSchemaError, SectionsEnvelope};
pub use site_pages::{SitePage, validate_page_slug};
pub use site_public::{PublishedSite, SitePublicStore};
pub use site_publish::{SitePageSnapshot, SitePublish};
pub use site_theme::{
    DEFAULT_THEME_PRESET, SiteTheme, THEME_PRESETS, THEME_SCHEMA_VERSION, ThemePreset,
    ThemeSchemaError, theme_preset,
};
pub use sites::{Site, SiteStatus, validate_subdomain};
pub use spaces::{Space, SpaceMember, SpaceRole};
pub use store::{CATEGORY_KEYWORD_PREFIX, SEEN, Store, TenantStore, category_keyword};
pub use tasks::{
    NewTask, Subtask, Task, TaskActivity, TaskComment, TaskDepRef, TaskEdit, TaskLabel, TaskProject,
};
