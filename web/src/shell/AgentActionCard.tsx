// The rich approval card for a proposed agent action (ADR 0034). The agent never
// acts on its own: it proposes exactly one action, and this card previews what it
// will do — recipient/subject/body for a draft, the target folder for a move, the
// email and time for a snooze — so the user approves with full sight of it. Send
// is the one outward, irreversible action and carries a caution note plus a "Send"
// label. Approving calls the execute route; discarding drops the proposal.
import {
  Archive,
  AlertTriangle,
  BellRing,
  CalendarPlus,
  CalendarRange,
  Clock,
  FileCheck,
  FileText,
  Flag,
  FlagOff,
  FolderInput,
  Gauge,
  Handshake,
  ListChecks,
  MailOpen,
  MessagesSquare,
  MoveRight,
  PackageSearch,
  PenLine,
  Percent,
  Reply,
  ScanSearch,
  Send,
  ShoppingCart,
  Sparkles,
  Tags,
  Trash2,
  UserMinus,
  type LucideIcon,
} from "lucide-react";

import { formatAmount } from "../billing";
import { getLocale, strings } from "../i18n";
import { durationLabel } from "../projects/format";
import { Button, Card, Spinner } from "../ds";
import type { AgentActionDto } from "../jmap";
import styles from "./AgentActionCard.module.css";

interface Field {
  label: string;
  value: string;
}

interface ActionView {
  icon: LucideIcon;
  title: string;
  fields: Field[];
  /** A multi-line body preview (a draft's text). */
  body?: string;
  /** A plain statement of what approving does — used where the action's name
   *  sounds bigger than it is (a billing draft is only ever a draft). */
  note?: string;
  /** Present only for an outward, irreversible action (send): a warning note. */
  caution?: string;
}

function str(args: Record<string, unknown>, key: string): string {
  const v = args[key];
  return typeof v === "string" ? v : "";
}

function isTrue(args: Record<string, unknown>, key: string): boolean {
  return args[key] === true;
}

/** Format an ISO datetime for display; falls back to the raw value if unparseable. */
/** A calendar day, for a tool that takes YYYY-MM-DD rather than an instant. */
function dayAt(day: string): string {
  if (day === "") return "";
  const at = new Date(`${day}T00:00:00`);
  if (Number.isNaN(at.getTime())) return day;
  return at.toLocaleDateString(getLocale(), {
    weekday: "long",
    day: "numeric",
    month: "long",
  });
}

function whenAt(iso: string): string {
  if (iso === "") return "";
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/** Format an ISO date (no time) for a task due date. */
function dayOf(iso: string): string {
  if (iso === "") return "";
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleDateString(undefined, { dateStyle: "medium" });
}

function subjectOrNone(args: Record<string, unknown>): string {
  return str(args, "subject") || strings.agentNoSubject;
}

/** The lines of a proposed invoice, each as "quantity × what" — what the user is
 *  approving, with no money in it: prices and totals belong to the document the
 *  server writes, and this card never formats or computes an amount. */
function proposedLines(args: Record<string, unknown>): string[] {
  const lines = args["lines"];
  if (!Array.isArray(lines)) return [];
  return lines.flatMap((line) => {
    if (typeof line !== "object" || line === null) return [];
    const l = line as Record<string, unknown>;
    const what = str(l, "product") || str(l, "description");
    if (what === "") return [];
    const qty = l["quantity"];
    const shown =
      typeof qty === "number" || typeof qty === "string" ? String(qty) : "1";
    return [`${shown} × ${what}`];
  });
}

/** What a proposed deal is worth, exactly as the proposal states it: whole cents,
 *  in the currency it names — and with no currency at all when it names none,
 *  rather than a symbol this card invented. Nothing is summed or converted; a
 *  value that is not a whole number of cents is not shown, because the server
 *  will refuse it rather than round it. */
function proposedValue(args: Record<string, unknown>): string {
  const cents = args["valueCents"];
  if (typeof cents !== "number" || !Number.isInteger(cents)) return "";
  const currency = str(args, "currency");
  return formatAmount(
    cents,
    getLocale(),
    currency === "" ? undefined : currency,
  );
}

/** Map a proposed action to its icon, title, and preview fields. Falls back to the
 *  model's own one-line `say` for any tool without a bespoke preview. */
function describeAction(action: AgentActionDto): ActionView {
  const a = action.args;
  const email: Field = {
    label: strings.agentFieldEmail,
    value: subjectOrNone(a),
  };
  switch (action.tool) {
    case "draft_email":
      return {
        icon: PenLine,
        title: strings.agentActDraft,
        fields: [
          { label: strings.agentFieldTo, value: str(a, "to") },
          { label: strings.agentFieldSubject, value: subjectOrNone(a) },
        ],
        body: str(a, "body"),
      };
    case "draft_reply": {
      const s = str(a, "subject");
      const re =
        s === "" ? strings.agentNoSubject : /^re:/i.test(s) ? s : `Re: ${s}`;
      return {
        icon: Reply,
        title: strings.agentActReply,
        fields: [{ label: strings.agentFieldReplyTo, value: re }],
        body: str(a, "body"),
      };
    }
    case "send_email":
      return {
        icon: Send,
        title: strings.agentActSend,
        fields: [{ label: strings.agentFieldSubject, value: subjectOrNone(a) }],
        caution: strings.agentSendCaution,
      };
    case "archive_email":
      return { icon: Archive, title: strings.agentActArchive, fields: [email] };
    case "trash_email":
      return { icon: Trash2, title: strings.agentActTrash, fields: [email] };
    case "mark_read":
      return {
        icon: MailOpen,
        title: isTrue(a, "read")
          ? strings.agentActMarkRead
          : strings.agentActMarkUnread,
        fields: [email],
      };
    case "flag_email":
      return {
        icon: isTrue(a, "flagged") ? Flag : FlagOff,
        title: isTrue(a, "flagged")
          ? strings.agentActFlag
          : strings.agentActUnflag,
        fields: [email],
      };
    case "snooze_email":
      return {
        icon: Clock,
        title: strings.agentActSnooze,
        fields: [
          email,
          { label: strings.agentFieldUntil, value: whenAt(str(a, "until")) },
        ],
      };
    case "move_to_folder":
      return {
        icon: FolderInput,
        title: strings.agentActMove,
        fields: [
          email,
          { label: strings.agentFieldFolder, value: str(a, "folder") },
        ],
      };
    case "create_task": {
      const fields: Field[] = [
        { label: strings.agentFieldTask, value: str(a, "title") },
      ];
      const due = dayOf(str(a, "due"));
      if (due !== "") fields.push({ label: strings.agentFieldDue, value: due });
      return { icon: ListChecks, title: strings.agentActTask, fields };
    }
    case "create_invoice_draft": {
      const lines = proposedLines(a);
      const fields: Field[] = [
        { label: strings.agentFieldCustomer, value: str(a, "customer") },
      ];
      if (lines.length > 0) {
        fields.push({
          label: strings.agentFieldLines,
          value: strings.agentLineCount(lines.length),
        });
      }
      return {
        icon: FileText,
        title: strings.agentActInvoiceDraft,
        fields,
        // What is on the document, without a single figure of money: the totals
        // are the server's, and this card never computes one.
        body: lines.join("\n"),
        note: strings.agentInvoiceDraftNote,
      };
    }
    case "quote_to_invoice":
      return {
        icon: FileCheck,
        title: strings.agentActQuoteToInvoice,
        fields: [{ label: strings.agentFieldQuote, value: str(a, "quote") }],
        note: strings.agentQuoteToInvoiceNote,
      };
    case "draft_payment_reminder":
      return {
        icon: BellRing,
        title: strings.agentActPaymentReminder,
        fields: [
          { label: strings.agentFieldInvoice, value: str(a, "invoice") },
        ],
        body: str(a, "note"),
        note: strings.agentReminderNote,
      };
    // CRM tools (ADR 0035, B2.10). A deal is named, never numbered, so every
    // one of these previews the title the user will be acting on.
    case "create_deal": {
      const fields: Field[] = [
        { label: strings.agentFieldDeal, value: str(a, "title") },
      ];
      const company = str(a, "company");
      if (company !== "")
        fields.push({ label: strings.agentFieldCompany, value: company });
      const value = proposedValue(a);
      if (value !== "") fields.push({ label: strings.agentFieldValue, value });
      const stage = str(a, "stage");
      if (stage !== "")
        fields.push({ label: strings.agentFieldStage, value: stage });
      // The note is only there when the proposal carries an email: approving
      // then links that conversation to the new deal, which is worth saying out
      // loud. (The propose route rewrites the source number into a message id,
      // so either shape means "raised from a conversation".)
      const fromEmail =
        a["source"] !== undefined || a["message_id"] !== undefined;
      return {
        icon: Handshake,
        title: strings.agentActCreateDeal,
        fields,
        ...(fromEmail ? { note: strings.agentDealFromEmailNote } : {}),
      };
    }
    case "move_deal_stage": {
      const fields: Field[] = [
        { label: strings.agentFieldDeal, value: str(a, "deal") },
        { label: strings.agentFieldStage, value: str(a, "stage") },
      ];
      const reason = str(a, "reason");
      if (reason !== "")
        fields.push({ label: strings.agentFieldLostReason, value: reason });
      return { icon: MoveRight, title: strings.agentActMoveDeal, fields };
    }
    case "draft_followup":
      return {
        icon: PenLine,
        title: strings.agentActFollowup,
        fields: [
          { label: strings.agentFieldDeal, value: str(a, "deal") },
          {
            label: strings.agentFieldSubject,
            value: str(a, "subject") || str(a, "deal"),
          },
        ],
        body: str(a, "body"),
        note: strings.agentFollowupNote,
      };
    // Projects tools (ADR 0035, B3.10a). A project is named, not numbered, and
    // a logged hour is a suggestion the timesheet accepts — both are said out
    // loud on the card, because approving is where the user decides.
    case "log_time": {
      const fields: Field[] = [
        { label: strings.agentFieldProject, value: str(a, "project") },
      ];
      const day = dayOf(str(a, "date"));
      if (day !== "") fields.push({ label: strings.agentFieldDay, value: day });
      const minutes = a["minutes"];
      if (typeof minutes === "number" && Number.isInteger(minutes)) {
        fields.push({
          label: strings.agentFieldDuration,
          value: durationLabel(minutes),
        });
      }
      return {
        icon: Clock,
        title: strings.agentActLogTime,
        fields,
        body: str(a, "note"),
        note: strings.agentLogTimeNote,
      };
    }
    case "project_status_summary":
      return {
        icon: Gauge,
        title: strings.agentActProjectStatus,
        fields: [
          { label: strings.agentFieldProject, value: str(a, "project") },
        ],
        note: strings.agentProjectStatusNote,
      };
    // The calendar draft (B3.10b). The days are what the user is really
    // approving — how many entries appear depends entirely on them — so the
    // range is a field of its own even when it is a single day.
    case "draft_timesheet_from_calendar": {
      const from = dayOf(str(a, "from") || str(a, "date"));
      const to = dayOf(str(a, "to")) || from;
      const fields: Field[] = [
        { label: strings.agentFieldProject, value: str(a, "project") },
      ];
      if (from !== "")
        fields.push({
          label: strings.agentFieldDay,
          value: strings.agentDraftedRange(from, to),
        });
      return {
        icon: CalendarRange,
        title: strings.agentActDraftTimesheet,
        fields,
        note: strings.agentDraftTimesheetNote,
      };
    }
    // The finance agent (ADR 0035, B4.14a). The user is approving a *period of
    // their own claims being looked at* — no category is on this card because
    // there is none to approve: the suggestions come from what they have
    // already agreed to, and each of them is answered afterwards, one at a time.
    case "categorise_transactions": {
      const from = dayOf(str(a, "from"));
      const to = dayOf(str(a, "to")) || from;
      const fields: Field[] = [];
      if (from !== "")
        fields.push({
          label: strings.agentCategoriseFieldPeriod,
          value: strings.agentDraftedRange(from, to),
        });
      return {
        icon: Tags,
        title: strings.agentActCategorise,
        fields,
        note: strings.agentCategoriseNote,
      };
    }
    // The finance agent's two answers (B4.14b). Both read and nothing else, so
    // the card previews the one thing the user is really approving: which days
    // are about to be read. The VAT card shows both days as the tool states
    // them — there is no default period to fall back on, and a figure under a
    // period nobody asked for is the one number that must never be guessed.
    case "vat_summary": {
      const from = dayOf(str(a, "from"));
      const to = dayOf(str(a, "to"));
      const fields: Field[] = [];
      if (from !== "" && to !== "")
        fields.push({
          label: strings.agentVatFieldPeriod,
          value: strings.agentDraftedRange(from, to),
        });
      return {
        icon: Percent,
        title: strings.agentActVatSummary,
        fields,
        note: strings.agentVatSummaryNote,
      };
    }
    case "flag_anomalies": {
      const from = dayOf(str(a, "from"));
      const to = dayOf(str(a, "to")) || from;
      const fields: Field[] = [];
      if (from !== "")
        fields.push({
          label: strings.agentAnomalyFieldPeriod,
          value: strings.agentDraftedRange(from, to),
        });
      return {
        icon: ScanSearch,
        title: strings.agentActFlagAnomalies,
        fields,
        note: strings.agentAnomalyNote,
      };
    }
    // The inventory agent (ADR 0035, B5.10). What the user is approving is a
    // *set of draft orders being written*, so the card previews the two
    // narrowings and nothing else — there is no quantity and no price on it
    // because there is none to approve: both come from the tenant's own
    // minimums, shelves and agreed price list.
    case "reorder_proposals": {
      const supplier = str(a, "supplier");
      const place = str(a, "location");
      return {
        icon: ShoppingCart,
        title: strings.agentActReorderProposals,
        fields: [
          {
            label: strings.agentFieldSupplier,
            value:
              supplier === "" ? strings.agentReorderEverySupplier : supplier,
          },
          {
            label: strings.agentFieldLocation,
            value: place === "" ? strings.agentReorderEverywhere : place,
          },
        ],
        note: strings.agentReorderNote,
      };
    }
    case "stock_answer":
      return {
        icon: PackageSearch,
        title: strings.agentActStockAnswer,
        fields: [
          { label: strings.agentFieldProduct, value: str(a, "product") },
        ],
        note: strings.agentStockAnswerNote,
      };
    case "create_event": {
      const fields: Field[] = [
        { label: strings.agentFieldEvent, value: str(a, "title") },
      ];
      const start = whenAt(str(a, "start"));
      if (start !== "")
        fields.push({ label: strings.agentFieldWhen, value: start });
      return { icon: CalendarPlus, title: strings.agentActEvent, fields };
    }
    // The reading tools. They change nothing, but they are still shown with
    // what they will look at: approving without seeing the arguments is
    // approving blind, and a card that merely repeats the sentence above it
    // has told the reader nothing they did not already know.
    case "whats_on": {
      const from = str(a, "from");
      const to = str(a, "to");
      return {
        icon: CalendarRange,
        title: strings.agentActWhatsOn,
        fields: [
          {
            label: strings.agentFieldWhen,
            value:
              to === "" || to === from
                ? dayAt(from)
                : `${dayAt(from)} — ${dayAt(to)}`,
          },
        ],
      };
    }
    case "am_i_free": {
      const start = whenAt(str(a, "start"));
      const end = whenAt(str(a, "end"));
      const fields: Field[] = [];
      if (start !== "")
        fields.push({
          label: strings.agentFieldWhen,
          value: end === "" ? start : `${start} — ${end}`,
        });
      return { icon: CalendarRange, title: strings.agentActAmIFree, fields };
    }
    case "catch_up_room":
      return {
        icon: MessagesSquare,
        title: strings.agentActCatchUp,
        fields: [
          { label: strings.agentFieldRoom, value: `#${str(a, "room")}` },
        ],
      };
    case "find_in_chat": {
      const fields: Field[] = [
        { label: strings.agentFieldLookingFor, value: str(a, "query") },
      ];
      const room = str(a, "room");
      if (room !== "")
        fields.push({ label: strings.agentFieldRoom, value: `#${room}` });
      return {
        icon: MessagesSquare,
        title: strings.agentActFindInChat,
        fields,
      };
    }
    case "find_file":
      return {
        icon: FileText,
        title: strings.agentActFindFile,
        fields: [
          { label: strings.agentFieldLookingFor, value: str(a, "query") },
        ],
      };
    case "find_contact":
      return {
        icon: Handshake,
        title: strings.agentActFindContact,
        fields: [
          { label: strings.agentFieldLookingFor, value: str(a, "query") },
        ],
      };
    // The HR agent's one read (B6.09). The days are shown because they are the
    // whole of what is being approved — "who is off" over the wrong week is a
    // different question — and the note says what the answer will and will not
    // contain, before it is read rather than after.
    case "who_is_off": {
      const from = str(a, "from");
      const to = str(a, "to");
      return {
        icon: UserMinus,
        title: strings.agentActWhoIsOff,
        fields: [
          {
            label: strings.agentFieldWhen,
            value:
              to === "" || to === from
                ? dayAt(from)
                : `${dayAt(from)} — ${dayAt(to)}`,
          },
        ],
        note: strings.agentWhoIsOffNote,
      };
    }
    default:
      return {
        icon: Sparkles,
        title: action.say || strings.agentProposedAction,
        fields: [],
      };
  }
}

export function AgentActionCard({
  action,
  running,
  onApprove,
  onDiscard,
  standing,
}: {
  action: AgentActionDto;
  running: boolean;
  onApprove: () => void;
  onDiscard: () => void;
  /** When set, the card shows the proposal but offers no decision, and says
   *  why. Used in chat, where a whole room sees a proposal only its asker may
   *  decide, and where a settled one stays visible as a record. Omitted
   *  everywhere the viewer is the only person who could ever decide. */
  standing?: { decidable: false; reason: string };
}) {
  const view = describeAction(action);
  const Icon = view.icon;
  const isSend = view.caution !== undefined;
  const hasPreview =
    view.fields.length > 0 || (view.body !== undefined && view.body !== "");
  return (
    <Card pad="sm" flat className={styles.stack}>
      <div className={styles.header}>
        <span className={styles.iconWrap} aria-hidden="true">
          <Icon size={16} />
        </span>
        <span className={styles.title}>{view.title}</span>
      </div>
      {hasPreview && (
        <div className={styles.preview}>
          {view.fields.map((f) => (
            <div key={f.label} className={styles.fact}>
              <span className={styles.factLabel}>{f.label}</span>
              <span className={styles.factValue}>
                {f.value === "" ? "—" : f.value}
              </span>
            </div>
          ))}
          {view.body !== undefined && view.body !== "" && (
            <p className={styles.body}>{view.body}</p>
          )}
        </div>
      )}
      {view.note !== undefined && <p className={styles.note}>{view.note}</p>}
      {view.caution !== undefined && (
        <p className={styles.caution}>
          <AlertTriangle size={14} aria-hidden />
          <span>{view.caution}</span>
        </p>
      )}
      {standing !== undefined ? (
        // Not a disabled button: a control that cannot be used should not be
        // shown as one. The sentence is the whole affordance.
        <p className={styles.note}>{standing.reason}</p>
      ) : (
        <div className={styles.decide}>
          <Button size="sm" onClick={onApprove} disabled={running}>
            {running ? (
              <Spinner size={14} />
            ) : isSend ? (
              strings.agentSendButton
            ) : (
              strings.agentApprove
            )}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={onDiscard}
            disabled={running}
          >
            {strings.agentDiscard}
          </Button>
        </div>
      )}
    </Card>
  );
}
