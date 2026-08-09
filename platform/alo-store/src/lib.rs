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
pub mod bank_camt;
pub mod bank_csv;
pub mod bank_import;
pub mod bank_match;
pub mod bank_mt940;
pub mod bank_read;
pub mod bank_reconcile;
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
pub mod chat;
pub mod chat_agents;
pub mod chat_attachments;
pub mod chat_mentions;
pub mod chat_messages;
pub mod chat_proposals;
pub mod chat_reactions;
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
pub mod fin_accounts;
pub mod fin_booking;
pub mod fin_categories;
pub mod fin_expenses;
pub mod fin_journal;
pub mod fin_ledger;
pub mod fin_mileage;
pub mod fin_receipt;
pub mod fin_receipt_read;
pub mod fin_rules;
pub mod iban;
pub mod ical;
pub mod id;
pub mod identity;
pub mod insight_catalog;
pub mod insight_dashboards;
pub mod insight_overview;
pub mod insight_prompt;
pub mod insight_query;
pub mod insight_series;
pub mod insight_spec;
pub mod insight_tiles;
pub mod maintenance;
pub mod message;
pub mod model;
pub mod money_text;
pub mod project_clients;
pub mod project_hours;
pub mod project_milestones;
pub mod project_templates;
pub mod reset;
pub mod rfc2047;
pub mod schedule;
pub mod search;
pub mod settings;
pub mod share;
pub mod signup;
pub mod site_assets;
pub mod site_form_notify;
pub mod site_forms;
pub mod site_model;
pub mod site_pages;
pub mod site_posts;
pub mod site_public;
mod site_public_analytics;
pub mod site_public_forms;
pub mod site_publish;
pub mod site_theme;
pub mod sites;
pub mod snooze;
pub mod spaces;
pub mod store;
pub mod tasks;
pub mod thread;
pub mod time_entries;
pub mod time_hours;
pub mod time_invoice;
pub mod time_report;
pub mod time_timer;
pub mod time_weeks;
pub mod vat_id;
pub mod vcard;

pub use account::AccountStore;
pub use account_imap::{ImapEntry, ImapMailbox, ImapSearchRow};
pub use account_sieve::{OutboundAction, SieveDelivery, SieveScriptMeta};
pub use bank_camt::parse_camt053;
pub use bank_csv::{BankCsvDates, BankCsvDecimal, BankCsvMapping};
pub use bank_import::{
    BANK_LINES_PAGE_MAX, BANK_REF_MAX, BankImport, BankLine, BankLineStatus, BankSource,
    BankStatement, COUNTERPARTY_NAME_MAX, LINE_AMOUNT_MAX_CENTS, MAX_BANK_FILE_BYTES, ParsedLine,
    ParsedStatement, REMITTANCE_MAX, STATEMENT_LINES_MAX, STATEMENT_REF_MAX,
};
pub use bank_match::{
    EXACT_WINDOW_DAYS, ExactMatch, MatchCandidate, NUMBERS_PER_REMITTANCE_MAX, document_numbers,
    ensure_exact_match, exact_match,
};
pub use bank_mt940::parse_mt940;
pub use bank_read::{
    BankFileImport, BankFileReading, BankImportRequest, read_bank_file, sniff_bank_source,
};
pub use bank_reconcile::{
    BANK_MATCH_METHOD, BankMatch, BankMatchTarget, BankSuggestions, ConfirmedMatch,
    LineSuggestions, SUGGESTION_NUMBERS_MAX,
};
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
pub use chat::{ChannelKind, ChannelVisibility, ChatChannel, ChatMember, MemberRole};
pub use chat_agents::{ChatAgent, ChatProposal, ProposalState};
pub use chat_attachments::{ATTACHMENTS_MAX, ChatAttachment};
pub use chat_mentions::parse_handles;
pub use chat_messages::{
    ChatChannelSummary, ChatFeedMessage, ChatMessage, MESSAGE_PAGE_DEFAULT, MessageKind,
};
pub use chat_reactions::{REACTIONS, ReactionTally};
pub use contacts::AddressHeaders;
pub use control::PLATFORM_TENANT_NAME;
pub use crm_activities::{Activity, ActivityKind, NewActivity};
pub use crm_deal_threads::{DealThread, ThreadSuggestion};
pub use crm_deals::{Deal, DealFilter, DealState, NewDeal, StageEvent, StageMove};
pub use crm_handoff::DealHandoff;
pub use crm_lead_import::{
    DuplicateReason, DuplicateRow, DuplicateSource, LeadImportReport, LeadImportRequest,
    LeadMapping, LeadRow,
};
pub use crm_next_steps::DEAL_SOURCE_KIND;
pub use crm_pipelines::{NewPipeline, Pipeline, PipelineSeed, StageSeed};
pub use crm_report::{PipelineCurrency, PipelineReport, PipelineStageRow, PipelineTally};
pub use crm_stages::{NewStage, Stage};
pub use crm_thread_match::MatchReason;
pub use csv_read::{CsvEncoding, CsvRow, CsvTable, RowError};
pub use dkim::DkimSigningMaterial;
pub use dmarc_reports::{DmarcAggregateRow, DmarcEventRecord};
pub use document::{Document, DocumentSummary};
pub use drive::{DriveLocation, DriveNode, DriveVersion, NewDriveFile};
pub use error::{Result, StoreError};
pub use fin_accounts::{
    ACCOUNT_CODE_MAX_CHARS, ACCOUNT_NAME_MAX_CHARS, Account, AccountRole, AccountType, CHART,
    CHART_SEED_KEY, ChartAccount, ChartName, ChartSeed, NewAccount,
};
pub use fin_categories::{CATEGORY_NAME_MAX_CHARS, ExpenseCategory, NewExpenseCategory};
pub use fin_expenses::{
    EXPENSE_DECISION_NOTE_MAX, EXPENSE_DESCRIPTION_MAX, Expense, ExpenseDecision, ExpenseMethod,
    ExpenseStatus, GROSS_MIN_CENTS, MERCHANT_MAX, NewExpense, PendingExpense,
};
pub use fin_journal::{
    DIMENSION_MAX_CHARS, ENTRY_POSTINGS_MAX, Entry, EntryKind, EntrySource, JOURNAL_PAGE_MAX,
    JournalEntry, MEMO_MAX_CHARS, NewEntry, NewPosting, POSTING_AMOUNT_MAX_CENTS, Posting,
    SourceEvent, SourceKind,
};
pub use fin_ledger::{
    AccountBalance, AccountLedger, DimensionBalance, DimensionBalances, LEDGER_GROUPS_MAX,
    LEDGER_PAGE_MAX, LedgerDimension, LedgerLine, LedgerScope, TrialBalance,
};
pub use fin_mileage::{
    KM_MAX_MILLI, KM_MIN_MILLI, MILEAGE_REASON_MAX, Mileage, MileageClaim, MileageRate, NewMileage,
    NewMileageRate, PLACE_MAX, RATE_MAX_CENTS_PER_KM, RATE_MIN_CENTS_PER_KM, RATE_NOTE_MAX,
    RATES_MAX, allowance_cents, rate_effective_on,
};
pub use fin_receipt::{
    AMOUNT_MAX_CENTS, Confidence, Evidence, Found, ParsedReceipt, PatternExtractor,
    RECEIPT_LINES_MAX, ReceiptExtractor, ReceiptInput, default_extractor,
};
pub use fin_receipt_read::{MAX_RECEIPT_BYTES, ReceiptReading};
pub use fin_rules::{
    InvoiceAccounts, PaymentAccounts, credit_note_entry, credit_note_original, invoice_issue_entry,
    payment_settle_entry, payment_settlement_role, settlement_needs_exchange_account,
};
pub use id::{
    AttachmentId, BankLineId, BankMatchId, BankStatementId, BaseFieldId, BaseRecordId, BaseTableId,
    BaseViewId, BillingBillId, BillingCustomerId, BillingInvoiceId, BillingLineId,
    BillingPaymentId, BillingProductId, BillingQuoteId, BillingScheduleId, BlobId, CalendarId,
    CategoryId, ChatAgentId, ChatChannelId, ChatMessageId, ChatProposalId, CommentId, ContactId,
    CrmActivityId, CrmDealId, CrmEventId, CrmPipelineId, CrmStageId, DriveNodeId, EventId,
    FinAccountId, FinCategoryId, FinEntryId, FinExpenseId, FinMileageId, FinMileageRateId,
    FinPostingId, GroupId, InsightDashboardId, InsightTileId, LabelId, MailboxId, MessageId,
    ProjectId, ProjectMilestoneId, SiteFormId, SiteFormSubmissionId, SiteId, SitePageId,
    SitePostId, SitePublishId, SpaceId, SubtaskId, TaskId, TenantId, ThreadId, TimeEntryId,
    TimeWeekId, UserId,
};
pub use identity::{
    AccessTokenRow, AuthCodeOutcome, AuthCodeRow, CredentialRow, OAuthClient, PublicKeyRow,
    RefreshTokenRow, SigningKeyRow, TotpRow,
};
pub use insight_catalog::{
    Aggregate, Dataset, DatasetEntry, Dimension, DimensionEntry, DimensionKind, FilterEntry,
    FilterField, FilterOp, Grain, Measure, MeasureEntry, Unit, ValueKind, Viz,
};
pub use insight_dashboards::{BUSINESS_OVERVIEW_KEY, Dashboard, NewDashboard};
pub use insight_overview::{
    BUSINESS_OVERVIEW, GALLERY, GalleryEntry, GalleryModule, OverviewCaption, OverviewSeed,
    gallery_entry,
};
pub use insight_prompt::catalog_prompt;
pub use insight_query::{MAX_GROUPS, MAX_SCANNED_ROWS};
pub use insight_series::{Label, Note, Point, Series, SeriesGroup, SeriesUnit};
pub use insight_spec::{
    ChartSpec, DimensionRef, Filter, MeasureRef, Period, Sort, SortBy, SortDir, SpecError,
};
pub use insight_tiles::{NewTile, Tile, TileSpec};
pub use model::{
    AiConfigRow, AiProviderRow, AuditEntry, Blob, Calendar, CalendarEvent, CalendarGrant, Category,
    Contact, ContactField, DkimKeyRow, DomainRow, EmailFilter, EmailQuery, GroupRow, MAX_PAGE,
    Mailbox, Message, MessageSummary, OccurrenceOverride, Page, SortDirection, TenantSummary,
    UserRow,
};
pub use project_clients::{BUDGET_CENTS_MAX, BUDGET_MINUTES_MAX, NewProjectClient, ProjectClient};
pub use project_hours::ProjectHours;
pub use project_milestones::{
    MILESTONES_MAX, Milestone, MilestoneEdit, NAME_MAX as MILESTONE_NAME_MAX, NewMilestone,
    TaskPlacement,
};
pub use project_templates::{
    PROJECT_NAME_MAX, ProjectTemplate, TEMPLATE_TASKS_MAX, TemplateCopy, TemplateInstance,
};
pub use reset::PendingReset;
pub use schedule::DueSend;
pub use search::SearchHit;
pub use share::{ShareCreated, ShareTarget};
pub use signup::PendingSignup;
pub use site_assets::{SITE_IMAGE_CONTENT_TYPES, SiteImageData, site_image_content_type};
pub use site_form_notify::FormNotification;
pub use site_forms::{
    MAX_FORMS_PER_SITE, SiteForm, SiteFormSubmission, SubmissionContent, normalize_submission,
};
pub use site_model::{SECTIONS_SCHEMA_VERSION, Section, SectionSchemaError, SectionsEnvelope};
pub use site_pages::{SitePage, validate_page_slug};
pub use site_posts::{NewSitePost, SitePost, SitePostStatus, SitePostUpdate};
pub use site_public::{
    PublishedSite, PublishedSitePost, PublishedSitePostBody, PublishedSitePostPage, SitePublicStore,
};
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
pub use time_entries::{
    MINUTES_MAX, MINUTES_MIN, NOTE_MAX as TIME_NOTE_MAX, NewTimeEntry, TimeEntry, TimeEntryEdit,
    TimeTotals, week_totals,
};
pub use time_hours::{hours_net_cents, qty_milli_hours};
pub use time_invoice::{
    MAX_HANDOFF_ENTRIES, TimeBilling, TimeInvoiceDraft, UnbilledCurrencyTotal, UnbilledGroup,
    UnbilledTotals, unbilled_totals,
};
pub use time_report::{
    ProfitabilityCurrency, ProfitabilityReport, ProfitabilityTotals, ProjectProfitability,
    profitability_totals,
};
pub use time_timer::{RunningTimer, StartTimer, StoppedTimer};
pub use time_weeks::{
    DECISION_NOTE_MAX, PendingWeek, TimesheetWeek, WeekDecision, WeekStatus, require_monday,
    week_end, week_start,
};
