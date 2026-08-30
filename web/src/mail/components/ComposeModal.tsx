// The compose window: a new message, reply, reply-all, or forward. Recipients
// are chips (To / Cc / Bcc), the quoted original is tucked behind a toggle, and
// on send it creates a draft (Email/set) then submits it (EmailSubmission/set),
// which sends it and files it to Sent. Bcc recipients are written into the
// sender's own copy but the server strips the Bcc header from the transmitted
// bytes, so they ride the envelope for delivery yet never appear to recipients.
import { useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent } from "react";
import {
  ArrowRight,
  ChevronDown,
  Clock,
  Link as LinkIcon,
  Maximize2,
  Minimize2,
  Minus,
  Paperclip,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";

import { strings } from "../../i18n";
import { IconButton, Select, Spinner, cx } from "../../ds";
import { useJmapClient } from "../../jmap";
import type { EmailAddress, EmailFull } from "../../jmap";
import { formatBytes, formatDate, mailErrorReason, senderName } from "../format";
import { htmlContent, textContent } from "../body";
import { RecipientInput } from "./RecipientInput";
import { RichTextEditor } from "./RichTextEditor";
import styles from "./ComposeModal.module.css";

export interface PendingAttachment {
  blobId: string;
  name: string;
  type: string;
  size: number;
  /** Message-part blobs must be copied through JMAP upload before reuse. */
  needsUpload?: boolean;
}

interface AttachmentTransferClient {
  downloadAttachment(blobId: string, name: string): Promise<Blob>;
  uploadFile(file: File): Promise<{ blobId: string; type: string; size: number }>;
}

/** Make attachment blobs reusable by Email/set. Existing message parts are
 * downloadable, but unlike freshly uploaded blobs they cannot be attached to
 * a newly created draft directly. */
export async function materializeAttachments(
  client: AttachmentTransferClient,
  attachments: PendingAttachment[],
): Promise<PendingAttachment[]> {
  return Promise.all(
    attachments.map(async (attachment) => {
      if (attachment.needsUpload !== true) return attachment;
      const bytes = await client.downloadAttachment(attachment.blobId, attachment.name);
      const uploaded = await client.uploadFile(
        new File([bytes], attachment.name, { type: attachment.type }),
      );
      return { ...attachment, ...uploaded, needsUpload: false };
    }),
  );
}

/** A large file sent as an expiring share link (alo Transfer) rather than an
 * inline attachment. */
interface LinkAttachment {
  url: string;
  name: string;
  size: number;
  expiresAt: number;
}

/** Files at or below this size attach inline; above it they upload as a share
 * link (sidestepping recipient attachment-size limits). There is no upper
 * limit — large files are streamed to storage. */
const ATTACH_MAX_BYTES = 25 * 1024 * 1024;

/** The share-link lifetime choices the sender can pick, in days. */
const EXPIRY_CHOICES = [1, 7, 30, 90] as const;

/** The download-link card appended to an outgoing message's HTML body. */
function linkCardHtml(link: LinkAttachment): string {
  const expires = new Date(link.expiresAt * 1000).toLocaleDateString();
  const name = escapeHtml(link.name);
  const url = escapeHtml(link.url);
  const meta = `${formatBytes(link.size)} · ${strings.transferExpires(expires)}`;
  return (
    `<div style="border:1px solid #dcd7cc;border-radius:10px;padding:14px 16px;margin-top:12px;max-width:420px;font-family:system-ui,-apple-system,sans-serif">` +
    `<div style="font-size:13px;color:#8a8578;margin-bottom:6px">${strings.transferSharedFile}</div>` +
    `<div style="font-weight:600;color:#102a43;word-break:break-all">${name}</div>` +
    `<div style="font-size:12px;color:#8a8578;margin:4px 0 10px">${escapeHtml(meta)}</div>` +
    `<a href="${url}" style="display:inline-block;background:#e76f51;color:#fff;text-decoration:none;padding:8px 14px;border-radius:8px;font-size:13px;font-weight:600">${strings.transferDownload}</a>` +
    `</div>`
  );
}

/** The plain-text fallback line for a share link. */
function linkCardText(link: LinkAttachment): string {
  const expires = new Date(link.expiresAt * 1000).toLocaleDateString();
  return `${link.name} (${formatBytes(link.size)}) — ${link.url} (${strings.transferExpires(expires)})`;
}

/** True when the composed HTML carries real formatting (not just line breaks),
 * so it's worth sending a text/html alternative. */
function hasFormatting(html: string): boolean {
  return (
    /<(?:b|strong|i|em|u|s|strike|a|ul|ol|li|h[1-6]|blockquote|pre|img|hr|font|span)\b/i.test(html) ||
    /style="/i.test(html) ||
    /data-alo-(?:latex|lang)=/i.test(html)
  );
}

/** Strip tags from a captured HTML fragment, returning its plain text. */
function stripTags(html: string): string {
  const el = document.createElement("div");
  el.innerHTML = html;
  return el.textContent ?? "";
}

/** Decode HTML entities in an attribute value (e.g. `&amp;` → `&`). */
function decodeAttr(value: string): string {
  const el = document.createElement("textarea");
  el.innerHTML = value;
  return el.value;
}

/**
 * A plain-text rendering of composed HTML, for the text/plain alternative. Math
 * and code blocks are reconstructed from their markers so a plain-text reader
 * still gets the LaTeX and fenced code, not stripped MathML glyphs.
 */
function htmlToText(html: string): string {
  const withBlocks = html
    // code blocks → fenced code
    .replace(
      /<pre\b[^>]*data-alo-lang="([^"]*)"[^>]*>([\s\S]*?)<\/pre>/gi,
      (_m, lang: string, inner: string) =>
        `\n\`\`\`${decodeAttr(lang)}\n${stripTags(inner)}\n\`\`\`\n`,
    )
    // display equations → LaTeX on its own line
    .replace(
      /<div\b[^>]*data-alo-latex="([^"]*)"[^>]*>[\s\S]*?<\/div>/gi,
      (_m, latex: string) => `\n${decodeAttr(latex)}\n`,
    )
    // inline equations → inline LaTeX
    .replace(
      /<span\b[^>]*data-alo-latex="([^"]*)"[^>]*>[\s\S]*?<\/span>/gi,
      (_m, latex: string) => ` ${decodeAttr(latex)} `,
    );
  const withBreaks = withBlocks
    .replace(/<\/(?:div|p|li|h[1-6]|blockquote)>/gi, "\n")
    .replace(/<br\s*\/?>/gi, "\n");
  const el = document.createElement("div");
  el.innerHTML = withBreaks;
  return (el.textContent ?? "").replace(/\n{3,}/g, "\n\n").trim();
}

/** Escape text for safe inclusion in an HTML body. */
function escapeHtml(text: string): string {
  const el = document.createElement("div");
  el.textContent = text;
  return el.innerHTML;
}

export interface ComposeContext {
  mode: "new" | "edit" | "reply" | "replyAll" | "forward";
  /** The source message for a reply or forward. */
  replyTo?: EmailFull;
  /** Seed the recipients (e.g. a mailto: unsubscribe address). New mode only. */
  to?: EmailAddress[];
  /** Seed the subject (e.g. "Fwd: …" for forward-as-attachment). */
  subject?: string;
  /** Seed the body (e.g. an AI smart-reply the user picked). */
  body?: string;
  /** Seed attachments, e.g. the original message as an .eml. */
  attachments?: { blobId: string; type: string; name: string; size: number }[];
}

/** A message queued for sending, handed to the parent so it can hold it during
 * the Undo window before actually submitting. */
export interface QueuedSend {
  emailId: string;
  fromEmail: string;
  rcpts: string[];
}

interface ComposeModalProps {
  context: ComposeContext;
  fromEmail: string;
  fromName: string;
  /** Addresses the user may send from (canonical + aliases). A From picker is
   * shown when there is more than one. */
  fromOptions: string[];
  draftsMailboxId: string | null;
  /** The user's signature (HTML) — inserted into the editable body. */
  signature: string;
  /** The tenant's organization footer (HTML) — appended after the signature. */
  orgFooter: string;
  onClose: () => void;
  /** Hand off a created draft to send after the Undo window. */
  onQueueSend: (queued: QueuedSend) => void;
  /** Hand off a created draft to be sent later at `sendAt` (Unix seconds). */
  onScheduleSend: (queued: QueuedSend & { sendAt: number }) => void;
}

/** The signature + org footer as an HTML block to seed the editor with, or ""
 * when both are empty. Two line breaks leave room to type above it. */
function signatureBlock(signature: string, orgFooter: string): string {
  const parts = [signature, orgFooter].map((s) => s.trim()).filter((s) => s.length > 0);
  return parts.length === 0 ? "" : `<br><br>${parts.join("<br>")}`;
}

/** Unix seconds for `hour:00` local time, `dayOffset` days from today. */
function atLocal(dayOffset: number, hour: number): number {
  const d = new Date();
  d.setDate(d.getDate() + dayOffset);
  d.setHours(hour, 0, 0, 0);
  return Math.floor(d.getTime() / 1000);
}

/** Epoch seconds → a `datetime-local` input value (`YYYY-MM-DDTHH:mm`) in the
 * user's local time, for the custom-time picker's `min`. */
function toLocalInputValue(epochSecs: number): string {
  const d = new Date(epochSecs * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Days from today until the next Monday (always ≥ 1, so "Monday" is never today). */
function daysUntilNextMonday(): number {
  const today = new Date().getDay(); // 0 = Sun … 1 = Mon
  const delta = (1 - today + 7) % 7;
  return delta === 0 ? 7 : delta;
}

/** The Gmail-style quick schedule presets, as (label, Unix-seconds) pairs. Times
 * beyond the workday roll to the next sensible slot via the day offsets. */
function schedulePresets(): { label: string; at: number }[] {
  return [
    { label: strings.scheduleTomorrowMorning, at: atLocal(1, 8) },
    { label: strings.scheduleTomorrowAfternoon, at: atLocal(1, 13) },
    { label: strings.scheduleMondayMorning, at: atLocal(daysUntilNextMonday(), 8) },
  ];
}

/** Format a future send time for the confirmation toast, in the user's locale. */
export function formatSendAt(epochSecs: number): string {
  return new Date(epochSecs * 1000).toLocaleString(undefined, {
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
    month: "short",
    day: "numeric",
  });
}

interface Prefill {
  to: EmailAddress[];
  cc: EmailAddress[];
  bcc: EmailAddress[];
  subject: string;
  /** The new message text (empty for a reply/forward — the user writes it). */
  body: string;
  /** The quoted original / forwarded block, shown behind a toggle and appended
   * to the body on send. */
  quoted: string;
  inReplyTo: string[];
  references: string[];
  showCc: boolean;
}

const EMPTY: Prefill = {
  to: [],
  cc: [],
  bcc: [],
  subject: "",
  body: "",
  quoted: "",
  inReplyTo: [],
  references: [],
  showCc: false,
};

/** Dedupe addresses, dropping empties and any already-seen (case-insensitive)
 * — `seen` is seeded with the addresses to exclude (e.g. the signed-in user). */
function collect(addrs: EmailAddress[], seen: Set<string>): EmailAddress[] {
  const out: EmailAddress[] = [];
  for (const a of addrs) {
    const key = a.email.trim().toLowerCase();
    if (key.length === 0 || seen.has(key)) continue;
    seen.add(key);
    out.push(a);
  }
  return out;
}

/** Reply-all recipients: To gets the sender plus every original To recipient;
 * Cc keeps the original Cc. The signed-in user (`me`) and any duplicate address
 * are removed throughout, and a Cc already present in To is dropped. Pure and
 * exported for testing. */
export function replyAllRecipients(
  source: Pick<EmailFull, "from" | "to" | "cc">,
  me: string,
): { to: EmailAddress[]; cc: EmailAddress[] } {
  const seen = new Set<string>([me.trim().toLowerCase()]);
  const to = collect([...(source.from ?? []), ...(source.to ?? [])], seen);
  const cc = collect(source.cc ?? [], seen);
  return { to, cc };
}

/** The "On <date>, <sender> wrote:" quoted-reply block. */
function quoteBlock(replyTo: EmailFull): string {
  const original = textContent(replyTo) ?? replyTo.preview;
  const header = `${formatDate(replyTo.receivedAt)} — ${senderName(replyTo)} ${strings.composeWroteOn}`;
  const quoted = original
    .split("\n")
    .map((line) => `> ${line}`)
    .join("\n");
  return `${header}\n${quoted}`;
}

/** The "---------- Forwarded message ----------" block. */
function forwardBlock(source: EmailFull): string {
  const original = textContent(source) ?? source.preview;
  const recipients = (source.to ?? [])
    .map((a) => (a.name !== null && a.name.length > 0 ? a.name : a.email))
    .join(", ");
  return [
    strings.composeForwardedIntro,
    `${strings.composeLabelFrom} ${senderName(source)} <${source.from?.[0]?.email ?? ""}>`,
    `${strings.composeLabelDate} ${formatDate(source.receivedAt)}`,
    `${strings.composeLabelSubject} ${source.subject ?? ""}`,
    `${strings.composeLabelTo} ${recipients}`,
    "",
    original,
  ].join("\n");
}

function stripRe(subject: string | null, prefix: RegExp): string {
  return (subject ?? "").replace(prefix, "");
}

export function buildPrefill(context: ComposeContext, me: string): Prefill {
  const src = context.replyTo;
  if (src === undefined) {
    // A fresh compose: honor any recipient/subject/body seeds (e.g. a mailto:
    // unsubscribe, or a forward-as-attachment subject).
    return {
      ...EMPTY,
      to: context.to ?? [],
      subject: context.subject ?? "",
      body: context.body ?? "",
    };
  }
  const threading = {
    inReplyTo: src.messageId ?? [],
    references: [...(src.references ?? []), ...(src.messageId ?? [])],
  };
  const firstFrom = src.from?.[0] !== undefined ? [src.from[0]] : [];

  if (context.mode === "edit") {
    const html = htmlContent(src);
    const text = textContent(src) ?? src.preview;
    return {
      ...EMPTY,
      to: src.to ?? [],
      cc: src.cc ?? [],
      bcc: src.bcc ?? [],
      showCc: (src.cc?.length ?? 0) > 0,
      subject: src.subject ?? "",
      body: html ?? escapeHtml(text).replace(/\n/g, "<br>"),
      inReplyTo: src.inReplyTo ?? [],
      references: src.references ?? [],
    };
  }

  if (context.mode === "reply") {
    return {
      ...EMPTY,
      ...threading,
      to: firstFrom,
      subject: strings.composeReplyPrefix + stripRe(src.subject, /^(re:\s*)+/i),
      body: context.body ?? "",
      quoted: quoteBlock(src),
    };
  }
  if (context.mode === "replyAll") {
    const { to, cc } = replyAllRecipients(src, me);
    return {
      ...EMPTY,
      ...threading,
      to,
      cc,
      showCc: cc.length > 0,
      subject: strings.composeReplyPrefix + stripRe(src.subject, /^(re:\s*)+/i),
      body: context.body ?? "",
      quoted: quoteBlock(src),
    };
  }
  if (context.mode === "forward") {
    // Forwarding starts a fresh conversation — no threading headers.
    return {
      ...EMPTY,
      subject: strings.composeForwardPrefix + stripRe(src.subject, /^(fwd:\s*)+/i),
      quoted: forwardBlock(src),
    };
  }
  // "new" — may carry a seeded subject (e.g. forward-as-attachment).
  return { ...EMPTY, subject: context.subject ?? "" };
}

function title(mode: ComposeContext["mode"]): string {
  switch (mode) {
    case "edit":
      return strings.composeEditTitle;
    case "reply":
      return strings.composeReplyTitle;
    case "replyAll":
      return strings.composeReplyAllTitle;
    case "forward":
      return strings.composeForwardTitle;
    default:
      return strings.composeTitle;
  }
}

export function ComposeModal({
  context,
  fromEmail,
  fromName,
  fromOptions,
  draftsMailboxId,
  signature,
  orgFooter,
  onClose,
  onQueueSend,
  onScheduleSend,
}: ComposeModalProps) {
  const client = useJmapClient();
  // The chosen From address (default: the signed-in address). A picker is
  // offered when the user holds more than one sendable address (aliases).
  const [from, setFrom] = useState(
    context.mode === "edit" ? (context.replyTo?.from?.[0]?.email ?? fromEmail) : fromEmail,
  );
  const prefill = useMemo(() => buildPrefill(context, fromEmail), [context, fromEmail]);
  // The signature block seeds the editor beneath the cursor. Used only as the
  // initial editor content (compose is opened after settings load).
  const initialBody = useMemo(
    () => prefill.body + (context.mode === "edit" ? "" : signatureBlock(signature, orgFooter)),
    [context.mode, prefill.body, signature, orgFooter],
  );
  const isReply = context.mode === "reply" || context.mode === "replyAll";
  const editingDraft = context.mode === "edit";

  const [to, setTo] = useState<EmailAddress[]>(prefill.to);
  const [cc, setCc] = useState<EmailAddress[]>(prefill.cc);
  const [bcc, setBcc] = useState<EmailAddress[]>(prefill.bcc);
  const [showCc, setShowCc] = useState(prefill.showCc);
  const [showBcc, setShowBcc] = useState(prefill.bcc.length > 0);
  const [subject, setSubject] = useState(prefill.subject);
  const [body, setBody] = useState(initialBody);
  // The editor is uncontrolled; `editorSeed` is what it mounts with and
  // `editorKey` remounts it when AI rewrites the whole draft.
  const [editorSeed, setEditorSeed] = useState(initialBody);
  const [editorKey, setEditorKey] = useState(0);
  const [aiEnabled, setAiEnabled] = useState(false);
  const [improving, setImproving] = useState(false);
  const [showQuoted, setShowQuoted] = useState(false);
  const [contacts, setContacts] = useState<EmailAddress[]>([]);

  useEffect(() => {
    let live = true;
    void client
      .aiEnabled()
      .then((on) => {
        if (live) setAiEnabled(on);
      })
      .catch(() => {
        // AI simply stays hidden if the session can't be read.
      });
    return () => {
      live = false;
    };
  }, [client]);

  // Recent correspondents for recipient autocomplete — fetched once when the
  // compose window opens; the fields filter this list locally as you type.
  useEffect(() => {
    let live = true;
    void client
      .recentContacts()
      .then((list) => {
        if (live) setContacts(list);
      })
      .catch(() => {
        // Autocomplete just stays empty if contacts can't be loaded.
      });
    return () => {
      live = false;
    };
  }, [client]);

  async function improve() {
    const draft = htmlToText(body);
    if (draft.trim().length === 0 || improving) return;
    setImproving(true);
    setError(null);
    try {
      const improved = await client.improveDraft(draft);
      const html = escapeHtml(improved).replace(/\n/g, "<br>");
      setEditorSeed(html);
      setBody(html);
      setEditorKey((k) => k + 1);
    } catch {
      setError(strings.aiImproveFailed);
    } finally {
      setImproving(false);
    }
  }
  // Gmail-style window states: docked bottom-right, minimized to its title bar,
  // or full-screen. Docked/minimized never block the mailbox behind them.
  const [view, setView] = useState<"normal" | "min" | "full">("normal");
  const minimized = view === "min";
  const [attachments, setAttachments] = useState<PendingAttachment[]>(
    context.attachments?.map((attachment) => ({ ...attachment, needsUpload: true })) ??
      (editingDraft
        ? (context.replyTo?.attachments ?? [])
            .filter((attachment) => attachment.cid === null)
            .map(({ blobId, name, type, size }) => ({ blobId, name, type, size, needsUpload: true }))
        : []),
  );
  const [links, setLinks] = useState<LinkAttachment[]>([]);
  const [linkExpiryDays, setLinkExpiryDays] = useState(7);
  const [uploading, setUploading] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [scheduleOpen, setScheduleOpen] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  async function onPickFiles(files: FileList) {
    setError(null);
    for (const file of Array.from(files)) {
      // Over the inline-attach size: send as an expiring share link (no upper
      // limit — it's streamed). Otherwise attach normally.
      setUploading((n) => n + 1);
      try {
        if (file.size > ATTACH_MAX_BYTES) {
          const share = await client.uploadShare(file, linkExpiryDays);
          setLinks((prev) => [
            ...prev,
            { url: share.url, name: share.filename, size: share.size, expiresAt: share.expiresAt },
          ]);
        } else {
          const up = await client.uploadFile(file);
          setAttachments((prev) => [
            ...prev,
            { blobId: up.blobId, name: file.name, type: up.type, size: up.size },
          ]);
        }
      } catch (error) {
        const reason = mailErrorReason(error);
        setError(
          reason === null ? strings.attachmentUploadFailed : strings.mailAttachmentErrorDetail(reason),
        );
      } finally {
        setUploading((n) => n - 1);
      }
    }
  }

  function removeAttachment(blobId: string) {
    setAttachments((prev) => prev.filter((a) => a.blobId !== blobId));
  }

  function removeLink(url: string) {
    setLinks((prev) => prev.filter((l) => l.url !== url));
  }

  const recipientTotal = useMemo(() => {
    const seen = new Set<string>();
    for (const a of [...to, ...cc, ...bcc]) seen.add(a.email.toLowerCase());
    return seen.size;
  }, [to, cc, bcc]);

  /** Validate, build the body, and create the draft. Returns the queued send
   * (draft id + envelope) or null on failure (error/sending state is set here).
   * Shared by immediate send and schedule-send so the two never diverge. */
  async function createDraftForSend(): Promise<QueuedSend | null> {
    if (to.length === 0 && cc.length === 0 && bcc.length === 0) {
      setError(strings.composeNoRecipients);
      return null;
    }
    if (draftsMailboxId === null) {
      setError(strings.composeSendError);
      return null;
    }
    setSending(true);
    setError(null);
    // The editor holds HTML; derive the text/plain alternative and append the
    // quoted original (as text, and as HTML when we're sending a formatted body).
    const text = htmlToText(body);
    // Large files ride the message as expiring share links (alo Transfer).
    const linkText = links.length > 0 ? `\n\n${links.map(linkCardText).join("\n")}` : "";
    const linkHtml = links.map(linkCardHtml).join("");
    const withQuote = prefill.quoted.length > 0 ? `${text}\n\n${prefill.quoted}` : text;
    const fullText = `${withQuote}${linkText}`;
    let bodyHtml: string | undefined;
    // Force an HTML part whenever there are link cards, even for a plain body.
    if (hasFormatting(body) || links.length > 0) {
      const quotedHtml =
        prefill.quoted.length > 0
          ? `<br><br><blockquote>${escapeHtml(prefill.quoted).replace(/\n/g, "<br>")}</blockquote>`
          : "";
      bodyHtml = `${body}${linkHtml}${quotedHtml}`;
    }
    try {
      const readyAttachments = await materializeAttachments(client, attachments);
      if (readyAttachments.some((attachment, index) => attachment !== attachments[index])) {
        setAttachments(readyAttachments);
      }
      const nextDraft = {
        mailboxId: draftsMailboxId,
        from: { name: fromName.length > 0 ? fromName : null, email: from },
        to,
        cc,
        bcc,
        subject,
        bodyText: fullText,
        ...(bodyHtml !== undefined ? { bodyHtml } : {}),
        inReplyTo: prefill.inReplyTo,
        references: prefill.references,
        attachments: readyAttachments.map((a) => ({ blobId: a.blobId, type: a.type, name: a.name })),
      };
      const emailId =
        editingDraft && context.replyTo !== undefined
          ? await client.replaceDraft(context.replyTo.id, nextDraft)
          : await client.createDraft(nextDraft);
      // Bcc is written into the draft so the sender's own Sent copy records who
      // was blind-copied; the server strips the Bcc header from the bytes it
      // transmits, so recipients never see it. Bcc addresses still ride the
      // envelope recipients here so they are actually delivered.
      const rcpts = [...to, ...cc, ...bcc].map((a) => a.email);
      return { emailId, fromEmail: from, rcpts };
    } catch (error) {
      const reason = mailErrorReason(error);
      setError(reason === null ? strings.composeSendError : strings.mailDraftCreateErrorDetail(reason));
      setSending(false);
      return null;
    }
  }

  async function onSend(event: FormEvent) {
    event.preventDefault();
    // The draft now exists; hand it to the parent, which holds it for the Undo
    // window and submits after. Undo just leaves it in Drafts.
    const queued = await createDraftForSend();
    if (queued !== null) onQueueSend(queued);
  }

  /** Schedule the composed message for `sendAt` (Unix seconds). Creates the
   * draft exactly as a normal send, then hands it to the parent to schedule. */
  async function onScheduleAt(sendAt: number) {
    setScheduleOpen(false);
    const queued = await createDraftForSend();
    if (queued !== null) onScheduleSend({ ...queued, sendAt });
  }

  return (
    <div className={cx(styles.host, styles[`host_${view}`])}>
      {view === "full" && <div className={styles.backdrop} />}
      <div
        className={cx(styles.window, styles[`window_${view}`])}
        role="dialog"
        aria-modal={view === "full"}
        aria-label={title(context.mode)}
      >
        <form onSubmit={onSend} className={styles.form}>
          <header
            className={styles.head}
            onClick={minimized ? () => setView("normal") : undefined}
            role={minimized ? "button" : undefined}
          >
            <h2 className={styles.title}>{title(context.mode)}</h2>
            {recipientTotal > 0 && (
              <span className={styles.countPill}>{strings.recipientCount(recipientTotal)}</span>
            )}
            <div className={styles.headSpacer} />
            <IconButton
              label={minimized ? strings.composeRestore : strings.composeMinimize}
              icon={<Minus />}
              onClick={(e) => {
                e.stopPropagation();
                setView(minimized ? "normal" : "min");
              }}
            />
            <IconButton
              label={view === "full" ? strings.composeCollapse : strings.composeExpand}
              icon={view === "full" ? <Minimize2 /> : <Maximize2 />}
              onClick={(e) => {
                e.stopPropagation();
                setView(view === "full" ? "normal" : "full");
              }}
            />
            <IconButton
              label={strings.composeDiscard}
              icon={<X />}
              onClick={(e) => {
                e.stopPropagation();
                onClose();
              }}
            />
          </header>

          <div className={styles.headers}>
            {fromOptions.length > 1 && (
              <div className={styles.fromRow}>
                <span className={styles.fromLabel}>{strings.composeFrom}</span>
                <Select
                  fullWidth
                  className={styles.fromPicker}
                  value={from}
                  onChange={(e) => setFrom(e.target.value)}
                  aria-label={strings.composeFrom}
                >
                  {fromOptions.map((addr) => (
                    <option key={addr} value={addr}>
                      {addr}
                    </option>
                  ))}
                </Select>
              </div>
            )}
            <RecipientInput
              label={strings.composeTo}
              value={to}
              onChange={setTo}
              suggestions={contacts}
              autoFocus={!isReply && !editingDraft}
              trailing={
                <>
                  {!showCc && (
                    <button type="button" className={styles.ccBtn} onClick={() => setShowCc(true)}>
                      {strings.composeCc}
                    </button>
                  )}
                  {!showBcc && (
                    <button type="button" className={styles.ccBtn} onClick={() => setShowBcc(true)}>
                      {strings.composeBcc}
                    </button>
                  )}
                </>
              }
            />
            {showCc && (
              <RecipientInput
                label={strings.composeCc}
                value={cc}
                onChange={setCc}
                suggestions={contacts}
              />
            )}
            {showBcc && (
              <RecipientInput
                label={strings.composeBcc}
                value={bcc}
                onChange={setBcc}
                suggestions={contacts}
              />
            )}
            <div className={styles.subjectRow}>
              <span className={styles.subjectLabel}>{strings.composeSubject}</span>
              <input
                className={styles.subjectInput}
                value={subject}
                onChange={(e) => setSubject(e.target.value)}
                placeholder={strings.composeSubjectPlaceholder}
              />
            </div>
          </div>

          <RichTextEditor
            key={editorKey}
            initialHtml={editorSeed}
            onChange={setBody}
            placeholder={strings.composeBodyPlaceholder}
            autoFocus={isReply || editingDraft}
          />

          {prefill.quoted.length > 0 && (
            <div className={styles.quotedWrap}>
              <button
                type="button"
                className={styles.quotedToggle}
                onClick={() => setShowQuoted((v) => !v)}
              >
                {showQuoted ? strings.hideQuoted : strings.showQuoted}
              </button>
              {showQuoted && <pre className={styles.quoted}>{prefill.quoted}</pre>}
            </div>
          )}

          {(attachments.length > 0 || uploading > 0) && (
            <div className={styles.attachRow}>
              {attachments.map((a) => (
                <span key={a.blobId} className={styles.attachChip}>
                  <Paperclip size={14} className={styles.attachIcon} />
                  <span className={styles.attachName}>{a.name}</span>
                  <span className={styles.attachSize}>{formatBytes(a.size)}</span>
                  <button
                    type="button"
                    className={styles.attachRemove}
                    onClick={() => removeAttachment(a.blobId)}
                    aria-label={strings.removeRecipient(a.name)}
                  >
                    <X size={13} />
                  </button>
                </span>
              ))}
              {uploading > 0 && (
                <span className={styles.attachChip}>
                  <Spinner size={14} />
                  <span className={styles.attachName}>{strings.attachmentUploading}</span>
                </span>
              )}
            </div>
          )}

          {links.length > 0 && (
            <div className={styles.attachRow}>
              {links.map((l) => (
                <span key={l.url} className={cx(styles.attachChip, styles.linkChip)}>
                  <LinkIcon size={14} className={styles.attachIcon} />
                  <span className={styles.attachName}>{l.name}</span>
                  <span className={styles.attachSize}>
                    {formatBytes(l.size)} · {strings.transferLink}
                  </span>
                  <button
                    type="button"
                    className={styles.attachRemove}
                    onClick={() => removeLink(l.url)}
                    aria-label={strings.removeRecipient(l.name)}
                  >
                    <X size={13} />
                  </button>
                </span>
              ))}
            </div>
          )}

          {error !== null && (
            <p className={styles.error} role="alert">
              {error}
            </p>
          )}

          <footer className={styles.footer}>
            <button
              type="button"
              className={styles.discard}
              onClick={onClose}
              aria-label={strings.composeDiscard}
            >
              <Trash2 size={17} />
            </button>
            <IconButton
              label={strings.attach}
              icon={<Paperclip />}
              onClick={() => fileInput.current?.click()}
            />
            <input
              ref={fileInput}
              type="file"
              multiple
              className={styles.fileInput}
              onChange={(e) => {
                if (e.target.files !== null && e.target.files.length > 0) {
                  void onPickFiles(e.target.files);
                }
                e.target.value = "";
              }}
            />
            <label className={styles.expirySelect} title={strings.transferExpiryTitle}>
              <Clock size={15} />
              <select
                value={linkExpiryDays}
                onChange={(e) => setLinkExpiryDays(Number(e.target.value))}
                aria-label={strings.transferExpiryTitle}
              >
                {EXPIRY_CHOICES.map((d) => (
                  <option key={d} value={d}>
                    {strings.transferExpiryOption(d)}
                  </option>
                ))}
              </select>
            </label>
            {aiEnabled && (
              <button
                type="button"
                className={styles.improve}
                onClick={() => void improve()}
                disabled={improving}
              >
                {improving ? <Spinner size={15} /> : <Sparkles size={15} />}
                <span>{strings.improve}</span>
              </button>
            )}
            <div className={styles.headSpacer} />
            <div className={styles.sendGroup}>
              <button type="submit" className={styles.send} disabled={sending || uploading > 0}>
                {sending ? (
                  <Spinner size={16} label={strings.composeSending} />
                ) : (
                  <>
                    <span>{strings.composeSend}</span>
                    <ArrowRight size={16} />
                  </>
                )}
              </button>
              <button
                type="button"
                className={styles.sendCaret}
                onClick={() => setScheduleOpen((v) => !v)}
                disabled={sending || uploading > 0}
                aria-haspopup="menu"
                aria-expanded={scheduleOpen}
                aria-label={strings.scheduleSend}
              >
                <ChevronDown size={16} />
              </button>
              {scheduleOpen && (
                <>
                  <button
                    type="button"
                    className={styles.scheduleScrim}
                    aria-hidden
                    tabIndex={-1}
                    onClick={() => setScheduleOpen(false)}
                  />
                  <div className={styles.schedulePop} role="menu">
                    <div className={styles.scheduleHead}>
                      <Clock size={14} />
                      <span>{strings.scheduleSend}</span>
                    </div>
                    {schedulePresets().map((preset) => (
                      <button
                        key={preset.label}
                        type="button"
                        role="menuitem"
                        className={`${styles.scheduleItem} hover:!bg-accent-soft hover:!text-accent`}
                        onClick={() => void onScheduleAt(preset.at)}
                      >
                        <span>{preset.label}</span>
                        <span className={styles.scheduleWhen}>{formatSendAt(preset.at)}</span>
                      </button>
                    ))}
                    <label className={styles.scheduleCustom}>
                      <span>{strings.schedulePickTime}</span>
                      <input
                        type="datetime-local"
                        className={styles.scheduleInput}
                        min={toLocalInputValue(atLocal(0, new Date().getHours() + 1))}
                        onChange={(e) => {
                          const ms = Date.parse(e.target.value);
                          if (!Number.isNaN(ms)) void onScheduleAt(Math.floor(ms / 1000));
                        }}
                      />
                    </label>
                  </div>
                </>
              )}
            </div>
          </footer>
        </form>
      </div>
    </div>
  );
}
