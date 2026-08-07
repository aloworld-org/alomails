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
  Clock,
  FileCheck,
  FileText,
  Flag,
  FlagOff,
  FolderInput,
  ListChecks,
  MailOpen,
  PenLine,
  Reply,
  Send,
  Sparkles,
  Trash2,
  type LucideIcon,
} from "lucide-react";

import { strings } from "../i18n";
import { Spinner } from "../ds";
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
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleDateString(undefined, { dateStyle: "medium" });
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
    const shown = typeof qty === "number" || typeof qty === "string" ? String(qty) : "1";
    return [`${shown} × ${what}`];
  });
}

/** Map a proposed action to its icon, title, and preview fields. Falls back to the
 *  model's own one-line `say` for any tool without a bespoke preview. */
function describeAction(action: AgentActionDto): ActionView {
  const a = action.args;
  const email: Field = { label: strings.agentFieldEmail, value: subjectOrNone(a) };
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
      const re = s === "" ? strings.agentNoSubject : /^re:/i.test(s) ? s : `Re: ${s}`;
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
        title: isTrue(a, "read") ? strings.agentActMarkRead : strings.agentActMarkUnread,
        fields: [email],
      };
    case "flag_email":
      return {
        icon: isTrue(a, "flagged") ? Flag : FlagOff,
        title: isTrue(a, "flagged") ? strings.agentActFlag : strings.agentActUnflag,
        fields: [email],
      };
    case "snooze_email":
      return {
        icon: Clock,
        title: strings.agentActSnooze,
        fields: [email, { label: strings.agentFieldUntil, value: whenAt(str(a, "until")) }],
      };
    case "move_to_folder":
      return {
        icon: FolderInput,
        title: strings.agentActMove,
        fields: [email, { label: strings.agentFieldFolder, value: str(a, "folder") }],
      };
    case "create_task": {
      const fields: Field[] = [{ label: strings.agentFieldTask, value: str(a, "title") }];
      const due = dayOf(str(a, "due"));
      if (due !== "") fields.push({ label: strings.agentFieldDue, value: due });
      return { icon: ListChecks, title: strings.agentActTask, fields };
    }
    case "create_invoice_draft": {
      const lines = proposedLines(a);
      const fields: Field[] = [{ label: strings.agentFieldCustomer, value: str(a, "customer") }];
      if (lines.length > 0) {
        fields.push({ label: strings.agentFieldLines, value: strings.agentLineCount(lines.length) });
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
        fields: [{ label: strings.agentFieldInvoice, value: str(a, "invoice") }],
        body: str(a, "note"),
        note: strings.agentReminderNote,
      };
    case "create_event": {
      const fields: Field[] = [{ label: strings.agentFieldEvent, value: str(a, "title") }];
      const start = whenAt(str(a, "start"));
      if (start !== "") fields.push({ label: strings.agentFieldWhen, value: start });
      return { icon: CalendarPlus, title: strings.agentActEvent, fields };
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
}: {
  action: AgentActionDto;
  running: boolean;
  onApprove: () => void;
  onDiscard: () => void;
}) {
  const view = describeAction(action);
  const Icon = view.icon;
  const isSend = view.caution !== undefined;
  const hasPreview = view.fields.length > 0 || (view.body !== undefined && view.body !== "");
  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <span className={styles.iconWrap}>
          <Icon size={16} />
        </span>
        <span className={styles.title}>{view.title}</span>
      </div>
      {hasPreview && (
        <div className={styles.preview}>
          {view.fields.map((f) => (
            <div key={f.label} className={styles.field}>
              <span className={styles.fieldLabel}>{f.label}</span>
              <span className={styles.fieldValue}>{f.value === "" ? "—" : f.value}</span>
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
      <div className={styles.buttons}>
        <button
          type="button"
          className={styles.approve}
          onClick={onApprove}
          disabled={running}
        >
          {running ? (
            <Spinner size={14} />
          ) : isSend ? (
            strings.agentSendButton
          ) : (
            strings.agentApprove
          )}
        </button>
        <button
          type="button"
          className={styles.discard}
          onClick={onDiscard}
          disabled={running}
        >
          {strings.agentDiscard}
        </button>
      </div>
    </div>
  );
}
