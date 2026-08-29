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
pub mod account_mapi;
pub mod account_sieve;
pub mod agent_goals;
pub mod agent_ground;
pub mod agent_instructions;
pub mod agent_memories;
pub mod agent_product;
pub mod agent_tool_runs;
pub mod app_passwords;
pub mod audit;
pub mod bank_camt;
pub mod bank_csv;
pub mod bank_ignore;
pub mod bank_import;
pub mod bank_manual;
pub mod bank_match;
pub mod bank_match_heuristic;
pub mod bank_mt940;
pub mod bank_read;
pub mod bank_reconcile;
pub mod bank_suggest;
pub mod bank_unmatch;
pub mod base;
pub mod billing_bills;
pub mod billing_cadence;
pub mod billing_catalog_read;
pub mod billing_cii_read;
pub mod billing_customers;
pub mod billing_demo;
pub mod billing_einvoice_import;
pub mod billing_field;
pub mod billing_fx;
pub mod billing_fx_ecb;
pub mod billing_fx_rates;
pub mod billing_invoices;
pub mod billing_line;
pub mod billing_payments;
pub mod billing_price_connections;
pub mod billing_products;
pub mod billing_quote_designs;
pub mod billing_quote_lines;
pub mod billing_quotes;
pub mod billing_schedules;
pub mod billing_sepa;
pub mod billing_sequence;
pub mod billing_settings;
pub mod billing_totals;
pub mod billing_ubl_read;
pub mod billing_vat;
pub mod billing_vat_report;
pub mod billing_xml_tree;
pub mod blob;
pub mod calendar;
pub mod calendar_availability;
pub mod calendar_resources;
pub mod calendar_working_hours;
pub mod campaign_audience;
pub mod campaign_bounce;
pub mod campaign_consent;
pub mod campaign_content;
pub mod campaign_dispatch;
pub mod campaign_html;
pub mod campaign_merge;
pub mod campaign_mime;
pub mod campaign_preview;
pub mod campaign_record;
pub mod campaign_segments;
pub mod campaign_send;
pub mod campaign_suppression;
pub mod campaign_text;
pub mod campaign_topic_optout;
pub mod campaign_unsubscribe;
pub mod campaign_unsubscribe_link;
pub mod campaign_warm_up;
pub mod changes;
pub mod chat;
pub mod chat_agent_dm;
pub mod chat_agent_seed;
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
pub mod crm_lead_capture;
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
pub mod events;
pub mod extract;
pub mod fin_accounts;
pub mod fin_aged;
pub mod fin_anomalies;
pub mod fin_balance;
pub mod fin_booking;
pub mod fin_categories;
pub mod fin_categorise;
pub mod fin_expenses;
pub mod fin_journal;
pub mod fin_ledger;
pub mod fin_match_rules;
pub mod fin_mileage;
pub mod fin_periods;
pub mod fin_pl;
pub mod fin_receipt;
pub mod fin_receipt_read;
pub mod fin_rules;
pub mod fin_vat_return;
pub mod hr_absences;
pub mod hr_applicants;
pub mod hr_checklists;
pub mod hr_documents;
pub mod hr_employees;
pub mod hr_employments;
pub mod hr_holiday_seed;
pub mod hr_holidays;
pub mod hr_leave_balances;
pub mod hr_leave_math;
pub mod hr_leave_policies;
pub mod hr_leave_requests;
pub mod hr_letters;
pub mod hr_openings;
pub mod hr_org;
pub mod hr_payroll_export;
pub mod hr_payroll_mapping;
pub mod hr_statutory_leave;
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
pub mod inv_adjust;
pub mod inv_barcode;
pub mod inv_count;
pub mod inv_count_apply;
pub mod inv_locations;
pub mod inv_moves;
pub mod inv_po;
pub mod inv_po_lines;
pub mod inv_po_receive;
pub mod inv_po_send;
pub mod inv_reorder;
pub mod inv_so;
pub mod inv_so_book;
mod inv_so_commit;
pub mod inv_so_confirm;
pub mod inv_so_deliver;
pub mod inv_so_invoice;
pub mod inv_so_lines;
pub mod inv_stock;
pub mod inv_stock_sale;
pub mod inv_supplier_prices;
pub mod inv_suppliers;
pub mod maintenance;
pub mod mapi_ids;
pub mod meet;
pub mod message;
pub mod mime_read;
pub mod mime_write;
pub mod model;
pub mod money_text;
pub mod project_clients;
pub mod project_hours;
pub mod project_milestones;
pub mod project_templates;
pub mod project_updates;
pub mod push_subscriptions;
pub mod record_origins;
pub mod reset;
pub mod rfc2047;
pub mod schedule;
pub mod search;
pub mod settings;
pub mod share;
pub mod signup;
pub mod site_agenda;
pub mod site_analytics;
pub mod site_assets;
pub mod site_attribution;
pub mod site_booking_manage;
pub mod site_booking_notify;
pub mod site_booking_publish;
pub mod site_booking_slots;
pub mod site_bookings;
pub mod site_catalog;
pub mod site_catalog_import;
pub mod site_catalog_items;
pub mod site_catalog_publish;
pub mod site_chat_actions;
pub mod site_chat_appearance;
pub mod site_chat_limits;
pub mod site_collections;
pub mod site_conversions;
pub mod site_custom_code;
pub mod site_domain_purchases;
pub mod site_domains;
pub mod site_editors;
pub mod site_form_notify;
pub mod site_forms;
pub mod site_generation;
pub mod site_grounding;
pub mod site_heatmap;
pub mod site_knowledge;
pub mod site_layout;
pub mod site_leads;
pub mod site_model;
pub mod site_order_notify;
pub mod site_orders;
pub mod site_page_protection;
pub mod site_pages;
pub mod site_payments;
pub mod site_payments_fixture;
pub mod site_posts;
pub mod site_public;
mod site_public_analytics;
pub mod site_public_bookings;
pub mod site_public_conversions;
pub mod site_public_forms;
pub mod site_public_heatmap;
pub mod site_public_leads;
pub mod site_public_orders;
pub mod site_public_protection;
pub mod site_public_shop;
pub mod site_public_stock;
pub mod site_public_tickets;
pub mod site_publish;
pub mod site_publish_schedule;
pub mod site_registrar;
pub mod site_registrar_fixture;
pub mod site_seed;
pub mod site_shop_items;
pub mod site_stock_fulfil;
pub mod site_stock_orders;
pub mod site_templates;
pub mod site_theme;
pub mod site_ticket_fulfil;
pub mod site_ticket_holds;
pub mod site_ticket_mail;
pub mod site_ticket_orders;
pub mod site_tickets;
pub mod site_translations;
pub mod site_versions;
pub mod sites;

pub use site_editors::{SiteEditorCollaborator, SiteEditorInviteOutcome, SiteEditorInviteTarget};
pub mod snooze;
pub mod spaces;
pub mod store;
pub mod tasks;
pub mod tenant_roles;
pub mod thread;
pub mod time_entries;
pub mod time_hours;
pub mod time_invoice;
pub mod time_report;
pub mod time_timer;
pub mod time_weeks;
pub mod tz;
pub mod user_invites;
pub mod user_modules;
pub mod vat_id;
pub mod vcard;

pub use account::AccountStore;
pub use account_imap::{ImapEntry, ImapMailbox, ImapSearchRow};
pub use account_mapi::{MapiDirectoryEntry, MapiMessageRow};
pub use account_sieve::{OutboundAction, SieveDelivery, SieveScriptMeta};
pub use agent_goals::{AgentGoal, GoalEnd, GoalStatus, GoalStep};
pub use agent_ground::GroundSource;
pub use agent_instructions::{
    AgentInstruction, DueInstruction, INSTRUCTION_MAX_MINUTES, INSTRUCTION_MIN_MINUTES,
    INSTRUCTION_TEXT_MAX, INSTRUCTIONS_PER_CHANNEL, InstructionTrigger,
};
pub use agent_memories::{AgentMemory, MEMORIES_PER_SCOPE, MEMORY_FACT_MAX, MemoryLearnedFrom};
pub use agent_product::{ALL_AGENT_PRODUCTS, AgentProduct};
pub use agent_tool_runs::{AgentToolRun, NewAgentToolRun};
pub use app_passwords::{
    APP_PASSWORD_NAME_MAX_CHARS, APP_PASSWORDS_MAX, AppPasswordCredential, AppPasswordRow,
};
pub use bank_camt::parse_camt053;
pub use bank_csv::{BankCsvDates, BankCsvDecimal, BankCsvMapping};
pub use bank_ignore::IGNORE_REASON_MAX_CHARS;
pub use bank_import::{
    BANK_LINES_PAGE_MAX, BANK_REF_MAX, BankImport, BankLine, BankLineStatus, BankSource,
    BankStatement, COUNTERPARTY_NAME_MAX, LINE_AMOUNT_MAX_CENTS, MAX_BANK_FILE_BYTES, ParsedLine,
    ParsedStatement, REMITTANCE_MAX, STATEMENT_LINES_MAX, STATEMENT_REF_MAX,
};
pub use bank_manual::ensure_manual_match;
pub use bank_match::{
    EXACT_WINDOW_DAYS, ExactMatch, MatchCandidate, NUMBERS_PER_REMITTANCE_MAX, document_numbers,
    ensure_exact_match, ensure_matchable, ensure_settleable, exact_match,
};
pub use bank_match_heuristic::{
    Candidate as HeuristicCandidate, LIKELY_MATCHES_MAX, LikelyMatch, MatchEvidence,
    NAME_SIMILAR_MIN_BP, SCORE_MIN, likely_matches, name_similarity_bp,
};
pub use bank_mt940::parse_mt940;
pub use bank_read::{
    BankFileImport, BankFileReading, BankImportRequest, read_bank_file, sniff_bank_source,
};
pub use bank_reconcile::{BANK_MATCH_METHOD, BankMatch, BankMatchTarget, ConfirmedMatch};
pub use bank_suggest::{BankSuggestions, LineSuggestions, OPEN_LEDGER_MAX, SUGGESTION_NUMBERS_MAX};
pub use bank_unmatch::UnmatchedLine;
pub use base::{Base, BaseField, BaseRecord, BaseTable, BaseView};
pub use billing_bills::{Bill, BillDocument, BillStatus, BillTotals, NewBill, Supplier};
pub use billing_cadence::{Cadence, next_occurrence};
pub use billing_catalog_read::{BillingCatalogRead, CatalogSaleItem};
pub use billing_customers::{Customer, NewCustomer};
pub use billing_einvoice_import::{EInvoiceSyntax, InboundInvoice, parse_einvoice};
pub use billing_fx::FxSnapshot;
pub use billing_fx_rates::{FxImport, FxRate, FxRateSource};
pub use billing_invoices::{Invoice, InvoiceDocument, InvoiceStatus, InvoiceSummary, NewInvoice};
pub use billing_line::{Line, NewLine};
pub use billing_payments::{NewPayment, Payment, PaymentState, Settlement};
pub use billing_products::{NewProduct, Product};
pub use billing_quote_designs::{QUOTE_DESIGN_MAX_BYTES, QuoteDesignRecord};
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
pub use calendar_availability::{CalendarAvailability, CalendarBusySpan, merged_busy_spans};
pub use calendar_resources::CalendarResource;
pub use calendar_working_hours::{WorkingHours, outside_hours_spans};
pub use campaign_audience::{
    AUDIENCE_PAGE_MAX, AudienceMember, AudiencePage, AudienceSource, CampaignRecipient,
    normalise_address,
};
pub use campaign_bounce::{BounceVerdict, CAMPAIGN_BOUNCE_MESSAGE_MAX, NewCampaignBounce};
pub use campaign_consent::{
    CONSENT_STATEMENT_MAX, CampaignConsent, ConsentEvidence, ConsentSource, NewCampaignConsent,
};
pub use campaign_content::{
    CAMPAIGN_BLOCK_ID_CHARS_MAX, CAMPAIGN_BLOCKS_MAX, CAMPAIGN_CELL_CHARS_MAX,
    CAMPAIGN_CONTENT_SCHEMA_VERSION, CAMPAIGN_HEADING_CHARS_MAX, CAMPAIGN_LANGUAGE_CHARS_MAX,
    CAMPAIGN_TABLE_COLUMNS_MAX, CAMPAIGN_TABLE_ROWS_MAX, CAMPAIGN_TEXT_CHARS_MAX, CampaignBlock,
    CampaignContent, CodeBlock, HeadingBlock, ParagraphBlock, TableBlock,
};
pub use campaign_html::{CAMPAIGN_LETTER_WIDTH_PX, CampaignLetter, render_campaign_html};
pub use campaign_merge::{
    CAMPAIGN_MERGE_FALLBACK_MAX, CampaignMergeField, CampaignMergeValues, PersonalisedLetter,
    ResolvedMergeField, personalise_campaign, reject_merge_fields, resolve_merge_content,
    resolve_merge_text, validate_merge_text,
};
pub use campaign_mime::{
    CAMPAIGN_HEADER_LINE_MAX, CAMPAIGN_MIME_LINE_MAX, CampaignMessage, render_campaign_message,
};
pub use campaign_preview::{CampaignPreview, FallbackReason, PreviewAgainst, PreviewAs};
pub use campaign_record::{
    CAMPAIGN_PAGE_MAX, CAMPAIGN_PREHEADER_MAX, CAMPAIGN_SUBJECT_MAX, Campaign, CampaignSummary,
    NewCampaign,
};
pub use campaign_segments::{
    CampaignSegment, ExclusionReason, NewCampaignSegment, PurchaseCondition, PurchaseWindow,
    SEGMENT_COUNTRIES_MAX, SEGMENT_NAME_MAX, SEGMENT_PAGE_MAX, SEGMENT_PERIOD_DAYS_MAX,
    SegmentConditions, SegmentExclusion, SegmentTally,
};
pub use campaign_suppression::{
    CampaignSuppression, NewSuppression, SUPPRESSION_PAGE_MAX, SuppressionEvidence,
    SuppressionReason,
};
pub use campaign_text::{CAMPAIGN_TEXT_WIDTH_COLS, render_campaign_text};
pub use campaign_topic_optout::{
    CampaignTopicOptOut, NewTopicOptOut, TOPIC_MAX, TOPIC_SOURCE_REF_MAX, normalise_topic,
};
pub use campaign_unsubscribe::{
    IssuedUnsubscribeToken, NewUnsubscribeToken, UNSUBSCRIBE_SEND_REF_MAX, UnsubscribeTokenTarget,
};
pub use changes::Changes;
pub use chat::{ChannelKind, ChannelVisibility, ChatChannel, ChatMember, MemberRole};
pub use chat_agent_seed::{
    AGENT_SEED_KEY, AgentSeed, AgentWords, LATER_AGENT_PRODUCTS, default_handle,
};
pub use chat_agents::{AgentRecord, ChatAgent, ChatProposal, ProposalState};
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
pub use crm_lead_capture::{CapturedLead, ConversationLead, CrmLeadCapture};
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
pub use events::{DomainEvent, NewDomainEvent, valid_event_name};
pub use fin_accounts::{
    ACCOUNT_CODE_MAX_CHARS, ACCOUNT_NAME_MAX_CHARS, Account, AccountRole, AccountType, CHART,
    CHART_SEED_KEY, ChartAccount, ChartName, ChartSeed, NewAccount,
};
pub use fin_aged::{
    AGED_BUCKETS, AgedBucket, AgedBuckets, AgedDocument, AgedParty, AgedReport, AgedSide,
};
pub use fin_anomalies::{
    ANOMALY_DUPLICATE, ANOMALY_FINDINGS_MAX, ANOMALY_MISSING_RECURRING, ANOMALY_SCAN_MAX,
    ANOMALY_SOURCES_MAX, ANOMALY_UNUSUAL_AMOUNT, Anomaly, AnomalyScan, AnomalySource, Counterparty,
    DUPLICATE_WINDOW_DAYS, OUTLIER_FACTOR, OUTLIER_FLOOR_CENTS, OUTLIER_MIN_SAMPLE, PARTY_CUSTOMER,
    PARTY_SUPPLIER, RECURRING_MIN_MONTHS, find_anomalies,
};
pub use fin_balance::{BalanceLine, BalanceSheet};
pub use fin_categories::{CATEGORY_NAME_MAX_CHARS, ExpenseCategory, NewExpenseCategory};
pub use fin_categorise::{
    CATEGORISE_CLAIMS_MAX, CATEGORISE_HISTORY_MAX, CategorisePlan, CategoryProposal,
    ClassifiedClaim, REASON_MERCHANT_HISTORY, SKIP_ALREADY_PROPOSED, SKIP_DECLINED,
    SKIP_NO_HISTORY, SKIP_NO_MERCHANT, SkippedClaim, merchant_key, plan_categorisation,
};
pub use fin_expenses::{
    EXPENSE_DECISION_NOTE_MAX, EXPENSE_DESCRIPTION_MAX, Expense, ExpenseDecision, ExpenseMethod,
    ExpenseStatus, GROSS_MIN_CENTS, MERCHANT_MAX, NewExpense, PendingExpense,
};
pub use fin_journal::{
    DIMENSION_MAX_CHARS, ENTRY_POSTINGS_MAX, Entry, EntryKind, EntrySource, JOURNAL_PAGE_MAX,
    JournalEntry, MEMO_MAX_CHARS, NewEntry, NewPosting, POSTING_AMOUNT_MAX_CENTS, Posting,
    SourceEvent, SourceKind, reversal_entry,
};
pub use fin_ledger::{
    AccountBalance, AccountLedger, DimensionBalance, DimensionBalances, LEDGER_GROUPS_MAX,
    LEDGER_PAGE_MAX, LedgerDimension, LedgerLine, LedgerScope, TrialBalance,
};
pub use fin_match_rules::{
    MatchOn, MatchRule, NewMatchRule, RULE_PATTERN_MAX, RULE_PATTERN_MIN, RULES_MAX,
};
pub use fin_mileage::{
    KM_MAX_MILLI, KM_MIN_MILLI, MILEAGE_REASON_MAX, Mileage, MileageClaim, MileageRate, NewMileage,
    NewMileageRate, PLACE_MAX, RATE_MAX_CENTS_PER_KM, RATE_MIN_CENTS_PER_KM, RATE_NOTE_MAX,
    RATES_MAX, allowance_cents, rate_effective_on,
};
pub use fin_periods::{
    ClosedThrough, FinPeriod, PERIOD_MAX_DAYS, PERIOD_NOTE_MAX_CHARS, PERIODS_MAX, PeriodStatus,
};
pub use fin_pl::{PlLine, ProfitAndLoss, comparative_period};
pub use fin_receipt::{
    AMOUNT_MAX_CENTS, Confidence, Evidence, Found, ParsedReceipt, PatternExtractor,
    RECEIPT_LINES_MAX, ReceiptExtractor, ReceiptInput, default_extractor,
};
pub use fin_receipt_read::{MAX_RECEIPT_BYTES, ReceiptReading};
pub use fin_rules::{
    InvoiceAccounts, PaymentAccounts, credit_note_entry, credit_note_original, invoice_issue_entry,
    payment_settle_entry, payment_settlement_role, settlement_needs_exchange_account,
};
pub use fin_vat_return::{VatReturn, VatReturnRate, VatReturnSide};
pub use hr_absences::{AbsenceDay, AbsentPerson};
pub use hr_applicants::{Applicant, ApplicantNote, ApplicantStage, NewApplicant};
pub use hr_checklists::{
    ChecklistKind, ChecklistOwners, ChecklistProgress, ChecklistRun, ChecklistStep,
    ChecklistTemplate, NewChecklistRun, NewChecklistStep, NewChecklistTemplate, PlannedStep,
    StepOwner,
};
pub use hr_documents::{HrDocument, HrDocumentKind};
pub use hr_employees::{DirectoryEntry, Employee, NewEmployee};
pub use hr_employments::{ContractKind, Employment, NewEmployment, PayPeriod};
pub use hr_holiday_seed::{Holiday, HolidayCalendar, holiday_calendar, holiday_calendars};
pub use hr_holidays::{HolidaySelection, TenantHolidays};
pub use hr_leave_balances::{PolicyBalance, fold_leave_year};
pub use hr_leave_math::{Accrual, Balance, LeaveLedger, LeaveYear, RequestCost, RequestedDay};
pub use hr_leave_policies::{LeaveKind, LeavePolicy, NewLeavePolicy};
pub use hr_leave_requests::{
    LeaveRequest, LeaveRequestQuery, LeaveStatus, NewLeaveRequest, leave_request_cost,
};
pub use hr_letters::{
    LetterFacts, LetterTemplate, MergeField, NewLetterTemplate, RenderedLetter, merge_fields,
    render_letter,
};
pub use hr_openings::{NewOpening, Opening, OpeningStatus};
pub use hr_org::{ORG_CHART_MAX_DEPTH, OrgNode, fold_org_chart};
pub use hr_payroll_export::{
    PAYROLL_PERIOD_MAX_DAYS, PayrollExport, PayrollLine, terms_for_period,
};
pub use hr_payroll_mapping::{
    ColumnMapping, MAPPINGS as PAYROLL_MAPPINGS, PayrollColumn, mapping as payroll_mapping,
    mapping_for_country as payroll_mapping_for_country,
};
pub use id::{
    AgentGoalId, AgentInstructionId, AgentMemoryId, AppPasswordId, AttachmentId, BankLineId,
    BankMatchId, BankStatementId, BaseFieldId, BaseRecordId, BaseTableId, BaseViewId,
    BillingBillId, BillingCustomerId, BillingInvoiceId, BillingLineId, BillingPaymentId,
    BillingPriceConnectionId, BillingProductId, BillingQuoteId, BillingScheduleId, BlobId,
    CalendarId, CampaignBounceId, CampaignConsentId, CampaignId, CampaignSegmentId, CampaignSendId,
    CampaignSuppressionId, CampaignUnsubscribeTokenId, CategoryId, ChatAgentId, ChatChannelId,
    ChatMessageId, ChatProposalId, ChatToolRunId, CommentId, ContactId, CrmActivityId, CrmDealId,
    CrmEventId, CrmPipelineId, CrmStageId, DomainEventId, DriveNodeId, EventId, FinAccountId,
    FinCategoryId, FinEntryId, FinExpenseId, FinMatchRuleId, FinMileageId, FinMileageRateId,
    FinPeriodId, FinPostingId, GroupId, HrApplicantId, HrApplicantNoteId, HrChecklistStepId,
    HrChecklistTemplateId, HrDocumentId, HrEmployeeId, HrEmploymentId, HrLeavePolicyId,
    HrLeaveRequestId, HrLetterTemplateId, HrOpeningId, HrPayrollExportId, InsightDashboardId,
    InsightTileId, InvCountId, InvLocationId, InvMoveId, InvPoReceiptId, InvPurchaseOrderId,
    InvReorderRuleId, InvSalesOrderId, InvSoDeliveryId, InvSoInvoiceId, InvStockHoldId,
    InvSupplierId, LabelId, MailboxId, MeetingId, MessageId, ProjectId, ProjectMilestoneId,
    PushSubscriptionId, SiteBookingAppointmentId, SiteBookingId, SiteCatalogCategoryId,
    SiteCatalogId, SiteCatalogItemId, SiteCollectionId, SiteDomainPurchaseId, SiteFormId,
    SiteFormSubmissionId, SiteId, SiteKnowledgeSourceId, SiteLeadLinkId, SiteOrderId, SitePageId,
    SitePostId, SitePublishId, SitePublishScheduleId, SiteShopItemId, SiteStockFulfilmentId,
    SiteStockOrderId, SiteTicketEventId, SiteTicketFulfilmentId, SiteTicketHoldId,
    SiteTicketOrderId, SpaceId, SubtaskId, TaskId, TenantId, ThreadId, TimeEntryId, TimeWeekId,
    UserId,
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
// The inventory modules are deliberately NOT re-exported here. `Supplier` is
// already the name of the party **copied onto a bill**
// (`billing_bills::Supplier`), and the master record is a different thing —
// the distinction the design note spends a paragraph on; `Location` and `Move`
// are words a mail-and-calendar codebase will want again for something else.
// Callers reach them by module path (`alo_store::inv_locations::NewLocation`,
// `alo_store::inv_moves::{NewMove, MoveReason}`, `alo_store::inv_stock::
// StockFilter`), which keeps every one of them unambiguous at its use site.
pub use meet::{
    Meeting, MeetingGuest, MeetingGuestInvitationCreated, MeetingMessage, MeetingMessageAttachment,
    MeetingMessageReaction, MeetingParticipant, MeetingRecording, MeetingRecordingConsent,
    MeetingTranscriptSegment, MeetingWorkspace, NewMeeting,
};
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
pub use project_updates::{
    ProjectUpdate, ProjectUpdateAttachment, ProjectUpdateState, UPDATE_BODY_MAX, UPDATE_LIST_MAX,
};
pub use push_subscriptions::{
    PUSH_ENDPOINT_MAX_CHARS, PUSH_KEY_MAX_CHARS, PUSH_SUBSCRIPTIONS_MAX, PushDelivery,
    PushSubscriptionRow,
};
pub use record_origins::RecordOrigin;
pub use reset::PendingReset;
pub use schedule::DueSend;
pub use search::SearchHit;
pub use settings::{IdentitySignature, OOO_HANDLE, OutOfOffice};
pub use share::{ShareCreated, ShareTarget};
pub use signup::PendingSignup;
pub use site_agenda::SiteAvailabilitySource;
pub use site_analytics::{
    SiteAnalyticsDay, SiteAnalyticsDimension, SiteAnalyticsRank, SiteAnalyticsReport,
};
pub use site_assets::{SITE_IMAGE_CONTENT_TYPES, SiteImageData, site_image_content_type};
pub use site_attribution::{SiteAttributionMoney, SiteAttributionReport, SiteAttributionSource};
pub use site_booking_manage::{CancelOutcome, ManagedAppointment};
pub use site_booking_notify::BookingNotification;
pub use site_booking_publish::SiteBookingSnapshot;
pub use site_booking_slots::{
    BookingRules, BookingSlot, BusyInterval, free_slots, local_day, local_wall_clock,
};
pub use site_bookings::{
    SITE_BOOKING_DESCRIPTION_MAX_CHARS, SITE_BOOKING_FIELD_KEY_MAX_CHARS,
    SITE_BOOKING_FIELD_LABEL_MAX_CHARS, SITE_BOOKING_FIELD_MAX_OPTIONS,
    SITE_BOOKING_FIELD_OPTION_MAX_CHARS, SITE_BOOKING_LOCATION_MAX_CHARS,
    SITE_BOOKING_MAX_BUFFER_MINUTES, SITE_BOOKING_MAX_DURATION_MINUTES, SITE_BOOKING_MAX_FIELDS,
    SITE_BOOKING_MAX_HORIZON_DAYS, SITE_BOOKING_MAX_NOTICE_MINUTES, SITE_BOOKING_MAX_PER_SITE,
    SITE_BOOKING_MAX_WINDOWS, SITE_BOOKING_MIN_DURATION_MINUTES, SITE_BOOKING_NAME_MAX_CHARS,
    SITE_BOOKING_TIME_ZONE_MAX_CHARS, SiteBooking, SiteBookingField, SiteBookingFieldKind,
    SiteBookingInput, SiteBookingWindow,
};
pub use site_catalog::{
    SITE_CATALOG_DESCRIPTION_MAX_CHARS, SITE_CATALOG_IMAGE_ALT_MAX_CHARS,
    SITE_CATALOG_MAX_CATEGORIES, SITE_CATALOG_MAX_ITEMS, SITE_CATALOG_MAX_PRICE_CENTS,
    SITE_CATALOG_NAME_MAX_CHARS, SITE_CATALOG_PRICE_NOTE_MAX_CHARS, SITE_CATALOG_SLUG_MAX_CHARS,
    SiteCatalog, SiteCatalogAvailability, SiteCatalogCategory, SiteCatalogCategoryInput,
    SiteCatalogInput, SiteCatalogItem, catalog_slug_from_name, currency_exponent,
    parse_price_minor_units, validate_currency,
};
pub use site_catalog_import::{
    SiteCatalogImport, SiteCatalogImportMapping, SiteCatalogImportReport,
};
pub use site_catalog_items::SiteCatalogItemInput;
pub use site_catalog_publish::{
    SiteCatalogSnapshot, SiteCatalogSnapshotCategory, SiteCatalogSnapshotItem,
};
pub use site_chat_actions::{
    CHAT_ACTIONS_KEPT, ChatActionCitation, NewChatAction, SiteChatAction, SiteChatActionKind,
};
pub use site_chat_appearance::{
    CHAT_APPEARANCE_SCHEMA_VERSION, CHAT_BOT_NAME_MAX_CHARS, CHAT_OFFLINE_MESSAGE_MAX_CHARS,
    CHAT_SUGGESTED_MAX, CHAT_SUGGESTED_QUESTION_MAX_CHARS, CHAT_TONE_NOTE_MAX_CHARS,
    CHAT_WELCOME_MAX_CHARS, ChatAppearanceError, ChatLauncherCorner, ChatLauncherIcon, ChatTone,
    ChatWidgetAccent, SiteChatAppearance,
};
pub use site_chat_limits::{
    ChatCeilingNotification, ChatGate, DEFAULT_CHAT_MONTHLY_CEILING_CENTS,
    MAX_CHAT_MONTHLY_CEILING_CENTS, MIN_CHAT_MONTHLY_CEILING_CENTS, SiteChatSettings,
    chat_month_key,
};
pub use site_collections::{
    SITE_COLLECTION_BODY_MAX_CHARS, SITE_COLLECTION_MAX_ITEMS, SITE_COLLECTION_NAME_MAX_CHARS,
    SITE_COLLECTION_TITLE_MAX_CHARS, SiteCollection, SiteCollectionFieldMapping,
    SiteCollectionInput, SiteCollectionItem, SiteCollectionSnapshot,
};
pub use site_conversions::{SiteConversionReport, SiteConversionSource};
pub use site_custom_code::{
    CustomCodeCapabilities, CustomCodeSection, MAX_CUSTOM_CODE_CSS_BYTES,
    MAX_CUSTOM_CODE_HEIGHT_PX, MAX_CUSTOM_CODE_HTML_BYTES, MAX_CUSTOM_CODE_JS_BYTES,
    MAX_CUSTOM_CODE_TOTAL_BYTES, MIN_CUSTOM_CODE_HEIGHT_PX,
};
pub use site_domain_purchases::{
    DueSiteDomainRegistration, MAX_SITE_DOMAIN_PURCHASES, NewSiteDomainPurchase,
    PAYMENT_REFERENCE_MAX, PAYMENT_REFERENCE_MIN, SITE_DOMAIN_PURCHASE_CLAIM_STALE_MINUTES,
    SITE_DOMAIN_PURCHASE_FAILURE_MAX_CHARS, SITE_DOMAIN_PURCHASE_INTERRUPTED,
    SITE_DOMAIN_PURCHASE_MAX_ATTEMPTS, SiteDomainPurchase, SiteDomainPurchaseKind,
    SiteDomainPurchaseState, validate_payment_reference,
};
pub use site_domains::{SITE_DOMAIN_MAX_LEN, SiteDomain, SiteDomainStatus, normalize_site_domain};
pub use site_form_notify::FormNotification;
pub use site_forms::{
    MAX_FORMS_PER_SITE, SiteForm, SiteFormSubmission, SubmissionContent, normalize_submission,
};
pub use site_generation::{GeneratedSiteDraft, NewGeneratedSite, NewGeneratedSitePage};
pub use site_grounding::{GroundingCitation, GroundingDocument, section_text};
pub use site_heatmap::{
    SiteHeatmapCell, SiteHeatmapPath, SiteHeatmapReport, SiteHeatmapScrollBucket,
    SiteHeatmapViewport,
};
pub use site_knowledge::{SITE_KNOWLEDGE_MAX_SOURCES, SiteKnowledgeSource};
pub use site_layout::{
    ColumnSplit, GridColumns, ImageShape, LayoutControl, RESIZABLE_SECTION_KINDS, layout_control,
    layout_controls,
};
pub use site_leads::{SiteLeadDeal, SiteLeadDraft, SiteLeadLink};
pub use site_model::{
    SECTION_KINDS, SECTIONS_SCHEMA_VERSION, Section, SectionSchemaError, SectionsEnvelope,
};
pub use site_order_notify::OrderNotification;
pub use site_orders::{
    ORDER_EMAIL_MAX_CHARS, ORDER_MAX_LINES, ORDER_MAX_QUANTITY, ORDER_NAME_MAX_CHARS,
    ORDER_NOTE_MAX_CHARS, ORDER_PHONE_MAX_CHARS, OrderContact, OrderRequestLine, SiteOrder,
    SiteOrderLine, SiteOrderStatus, normalize_order_contact, normalize_order_lines,
};
pub use site_page_protection::{
    SITE_PAGE_PASSWORD_MAX_CHARS, SITE_PAGE_PASSWORD_MIN_CHARS, SitePageProtection,
    validate_site_page_password,
};
pub use site_pages::{
    LocalizedSitePage, SiteLocaleReadiness, SitePage, SiteTranslationReadiness, validate_page_slug,
};
pub use site_payments::{
    SITE_PAYMENT_DESCRIPTION_MAX_CHARS, SITE_PAYMENT_MAX_AMOUNT_CENTS, SITE_PAYMENT_URL_MAX_CHARS,
    SitePaymentCreated, SitePaymentEnvironment, SitePaymentError, SitePaymentFuture,
    SitePaymentIdentity, SitePaymentProvider, SitePaymentRequest, SitePaymentResult,
    SitePaymentStatus, UnconfiguredSitePayments,
};
pub use site_payments_fixture::FixtureSitePayments;
pub use site_posts::{NewSitePost, SitePost, SitePostStatus, SitePostUpdate};
pub use site_public::{
    PublishedSite, PublishedSitePost, PublishedSitePostBody, PublishedSitePostPage, SitePublicStore,
};
pub use site_public_analytics::{
    DeviceClass, OUTBOUND_OVERFLOW, PublicSiteSignal, PublicSiteVisit, ReadTimeBucket,
};
pub use site_public_bookings::{
    BOOKING_ANSWER_MAX_CHARS, BOOKING_VISITOR_EMAIL_MAX_CHARS, BOOKING_VISITOR_NAME_MAX_CHARS,
    BookingAnswer, BookingRequest, PublicBookingService, ReservedAppointment,
};
pub use site_public_conversions::{
    CONVERSION_SOURCE_ID_MAX_LEN, ConversionSource, ConversionStage,
};
pub use site_public_heatmap::{
    HEATMAP_COLUMNS, HEATMAP_DAILY_PATHS, HEATMAP_ROWS, HeatmapCell, HeatmapSignal,
    PublicSiteHeatmapReport, SCROLL_DEPTH_BUCKETS, ScrollDepth, ViewportClass,
};
pub use site_public_protection::PublishedPageProtection;
pub use site_public_shop::{
    PublicTicketCheckout, PublicTicketEvent, PublicTicketOrderStatus, TICKET_CHECKOUT_HOLD_TTL,
};
pub use site_public_stock::{
    PublicStockCheckout, PublicStockItem, PublicStockOrderStatus, STOCK_CHECKOUT_HOLD_TTL,
};
pub use site_public_tickets::PublicTicket;
pub use site_publish::{SitePageSnapshot, SitePublish};
pub use site_publish_schedule::{
    DueSitePublish, MAX_SITE_PUBLISH_SCHEDULE_HISTORY, SITE_PUBLISH_CLAIM_STALE_MINUTES,
    SITE_PUBLISH_INTERRUPTED, SITE_PUBLISH_MAX_ATTEMPTS, SITE_PUBLISH_SCHEDULE_ERROR_MAX_CHARS,
    SITE_PUBLISH_SCHEDULE_MAX_AHEAD_DAYS, SitePublishSchedule, SitePublishScheduleStatus,
};
pub use site_registrar::{
    DomainAvailability, DomainCandidate, DomainLifecycle, DomainOffer, DomainOrder, DomainQuote,
    DomainRegistrar, DomainSearch, REGISTRAR_CURRENCY, RegisteredDomain, RegistrableDomain,
    RegistrantContact, RegistrarEnvironment, RegistrarError, RegistrarIdentity, RetailPolicy,
    TERM_YEARS_MAX, TERM_YEARS_MIN, TldCatalog, TldOffer, TldRequirement, UnconfiguredRegistrar,
};
pub use site_registrar_fixture::FixtureRegistrar;
pub use site_seed::{SectionSeed, SeedBinding, SeedContext, SeedNeed, SeedPage, seed_section};
pub use site_shop_items::{
    SHOP_SHIPPING_MAX_CENTS, SITE_SHOP_ITEM_MAX_PER_SITE, SiteShopCandidate, SiteShopItem,
    SiteShopShelfRow,
};
pub use site_stock_fulfil::{ClaimedStockFulfilment, StockFulfilWords, StockFulfilmentOutcome};
pub use site_stock_orders::{
    MAX_SITE_STOCK_ORDERS, STOCK_ORDER_GOODS_GONE, STOCK_ORDER_PAID_AFTER_LAPSE, ShipTo,
    SiteStockOrder, SiteStockOrderState, StockPaymentTarget,
};
pub use site_templates::{
    SiteTemplate, SiteTemplatePage, TEMPLATE_PLACEHOLDER_PRICE, check_template, site_template,
    site_templates,
};
pub use site_theme::{
    DEFAULT_THEME_PRESET, SiteTheme, THEME_PRESETS, THEME_SCHEMA_VERSION, ThemePreset,
    ThemeSchemaError, theme_preset,
};
pub use site_ticket_fulfil::{ClaimedTicketFulfilment, TicketFulfilWords, TicketFulfilmentOutcome};
pub use site_ticket_holds::{
    SiteTicketHold, SiteTicketHoldState, TICKET_HOLD_MAX_QUANTITY, TICKET_HOLD_MAX_TTL,
    TICKET_HOLD_MIN_TTL, TicketAvailability,
};
pub use site_ticket_mail::TicketMailNotification;
pub use site_ticket_orders::{
    MAX_SITE_TICKET_ORDERS, SiteTicketOrder, SiteTicketOrderState, TICKET_BUYER_EMAIL_MAX_CHARS,
    TICKET_BUYER_NAME_MAX_CHARS, TICKET_ORDER_PAID_AFTER_LAPSE, TicketPaymentTarget,
};
pub use site_tickets::{
    SITE_TICKET_EVENT_MAX_CAPACITY, SITE_TICKET_EVENT_MAX_PER_SITE, SiteTicketEvent,
    SiteTicketSeatCount,
};
pub use site_translations::{
    SiteTranslationPageContent, SiteTranslationPageWrite, SiteTranslationPostContent,
    SiteTranslationPostWrite,
};
pub use site_versions::{
    MAX_SITE_PUBLISH_HISTORY, SiteCollectionVersionChange, SitePageVersionChange,
    SitePageVersionField, SitePublishComparison, SitePublishVersion, SiteVersionChange,
};
pub use sites::{
    DEFAULT_SITE_LOCALE, MAX_SITE_LOCALES, Site, SiteStatus, normalize_locale_tag,
    normalize_site_locales, validate_subdomain,
};
pub use spaces::{Space, SpaceMember, SpaceRole};
pub use store::{CATEGORY_KEYWORD_PREFIX, SEEN, Store, TenantStore, category_keyword};
pub use tasks::{
    NewTask, ProjectWorkSummary, Subtask, Task, TaskActivity, TaskComment, TaskDepRef, TaskEdit,
    TaskLabel, TaskProject, TaskProjectEdit,
};
pub use tenant_roles::{AccessFacts, TenantRole};
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
    DECISION_NOTE_MAX, PendingProjectHours, PendingWeek, TimesheetWeek, WeekDecision, WeekStatus,
    require_monday, week_end, week_start,
};
pub use user_invites::{INVITE_TTL_DAYS, InviteStore, UserInviteTarget};
pub use user_modules::{ALL_MODULES, AppModule};
