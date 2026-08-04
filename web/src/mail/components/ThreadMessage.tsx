// One message within a conversation. Collapsed: a clickable summary row
// (avatar, sender, snippet, date). Expanded: the sender block plus the body —
// plain text in Garamond, HTML isolated in a sandboxed, CSP-locked iframe.
import { useEffect, useState } from "react";
import {
  ChevronDown,
  Download,
  File,
  FileArchive,
  FileImage,
  FileSpreadsheet,
  FileText,
  MoreHorizontal,
  Paperclip,
  ShieldCheck,
} from "lucide-react";

import { strings } from "../../i18n";
import { Avatar, Spinner, cx } from "../../ds";
import { useJmapClient } from "../../jmap";
import type { EmailAddress, EmailAttachment, EmailFull } from "../../jmap";
import { formatBytes, formatDate, senderName, subjectOr } from "../format";
import { htmlContent, sandboxedHtml, splitQuotedHtml, splitQuotedText, textContent } from "../body";
import { InvitationCard } from "./InvitationCard";
import styles from "./ThreadMessage.module.css";

function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("read failed"));
    reader.readAsDataURL(blob);
  });
}

/** Rewrites `cid:` image references in an HTML body to the resolved data-URIs,
 * so inline images render inside the sandboxed iframe (whose CSP already allows
 * `img-src data:`) without any remote fetch. */
function applyInlineImages(html: string, images: ReadonlyMap<string, string>): string {
  if (images.size === 0) return html;
  return html.replace(/cid:([^"'\s)>]+)/gi, (whole, id: string) => {
    return images.get(id.toLowerCase()) ?? whole;
  });
}

/** Fetches the message's inline parts (those carrying a Content-ID) and returns
 * a cid → data-URI map, so [`applyInlineImages`] can embed them. */
function useInlineImages(email: EmailFull, expanded: boolean): ReadonlyMap<string, string> {
  const client = useJmapClient();
  const [images, setImages] = useState<ReadonlyMap<string, string>>(new Map());
  useEffect(() => {
    if (!expanded) return;
    const inline = (email.attachments ?? []).filter((a) => a.cid !== null);
    if (inline.length === 0) return;
    let live = true;
    void (async () => {
      const entries: [string, string][] = [];
      for (const att of inline) {
        if (att.cid === null) continue;
        try {
          const blob = await client.downloadAttachment(att.blobId, att.name);
          entries.push([att.cid.toLowerCase(), await blobToDataUrl(blob)]);
        } catch {
          // A missing inline part just renders as a broken image — never fatal.
        }
      }
      if (live && entries.length > 0) setImages(new Map(entries));
    })();
    return () => {
      live = false;
    };
  }, [client, email.id, expanded]);
  return images;
}

/** Save a fetched Blob to the user's downloads with the given filename. */
function saveBlob(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

/** The file-type icon for an attachment name (Gmail shows one per card). */
function fileIcon(name: string) {
  const ext = name.slice(name.lastIndexOf(".") + 1).toLowerCase();
  if (["zip", "rar", "7z", "gz", "tar"].includes(ext)) return FileArchive;
  if (["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp"].includes(ext)) return FileImage;
  if (["xls", "xlsx", "csv", "ods"].includes(ext)) return FileSpreadsheet;
  if (["pdf", "doc", "docx", "odt", "txt", "rtf"].includes(ext)) return FileText;
  return File;
}

/** A Gmail-style attachment card: a file-type icon, the name and size, and a
 * download affordance. Fetches the bytes (authorized) on click. */
function AttachmentChip({ attachment }: { attachment: EmailAttachment }) {
  const client = useJmapClient();
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const Icon = fileIcon(attachment.name);

  async function download() {
    if (busy) return;
    setBusy(true);
    setFailed(false);
    try {
      const blob = await client.downloadAttachment(attachment.blobId, attachment.name);
      saveBlob(blob, attachment.name);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  }

  return (
    <button
      type="button"
      className={cx(styles.attachment, failed && styles.attachmentFailed)}
      onClick={download}
      disabled={busy}
      title={failed ? strings.attachmentFailed : strings.downloadAttachment(attachment.name)}
    >
      <span className={styles.attachIconBox}>
        <Icon className={styles.attachIcon} aria-hidden="true" />
      </span>
      <span className={styles.attachMeta}>
        <span className={styles.attachName}>{attachment.name}</span>
        <span className={styles.attachSize}>
          {failed
            ? strings.attachmentFailed
            : busy
              ? strings.attachmentDownloading
              : formatBytes(attachment.size)}
        </span>
      </span>
      {busy ? (
        <Spinner size={16} />
      ) : (
        <Download className={styles.attachDownload} aria-hidden="true" />
      )}
    </button>
  );
}

interface ThreadMessageProps {
  email: EmailFull;
  expanded: boolean;
  /** The signed-in user's address, so their own line reads "me". */
  me: string | undefined;
  onToggle: () => void;
}

function displayName(a: EmailAddress, me: string | undefined): string {
  if (me !== undefined && a.email.toLowerCase() === me) return "me";
  return a.name !== null && a.name.length > 0 ? a.name : a.email;
}

function recipientLine(to: EmailAddress[] | null, me: string | undefined): string {
  if (to === null || to.length === 0) return "";
  return to.map((a) => displayName(a, me)).join(", ");
}

/** True when inbound authentication passed strongly enough to vouch for the
 * sender: DMARC pass, or DKIM pass in the absence of a DMARC verdict. */
function isVerified(email: EmailFull): boolean {
  const auth = email["alo:authentication"];
  if (auth === undefined || auth === null) return false;
  if (auth.dmarc === "pass") return true;
  return auth.dkim === "pass" && (auth.dmarc === null || auth.dmarc === "none");
}

/** One "To / Cc / Bcc" row of the expanded recipient block; renders nothing
 * when the field is empty. Bcc is only ever populated on the sender's own copy. */
function RecipientRow({
  label,
  people,
  me,
}: {
  label: string;
  people: EmailAddress[] | null;
  me: string | undefined;
}) {
  if (people === null || people.length === 0) return null;
  return (
    <div className={styles.recipientRow}>
      <span className={styles.recipientLabel}>{label}</span>
      <span className={styles.recipientNames}>{recipientLine(people, me)}</span>
    </div>
  );
}

export function ThreadMessage({ email, expanded, me, onToggle }: ThreadMessageProps) {
  const [recipOpen, setRecipOpen] = useState(false);
  const [quotedOpen, setQuotedOpen] = useState(false);

  const rawText = expanded ? textContent(email) : null;
  const rawHtml = expanded && rawText === null ? htmlContent(email) : null;
  const textSplit = rawText !== null ? splitQuotedText(rawText) : null;
  const htmlSplit = rawHtml !== null ? splitQuotedHtml(rawHtml) : null;
  const inlineImages = useInlineImages(email, expanded);
  // Inline (cid-referenced) parts render embedded in the HTML, not as file chips.
  const attachments = expanded
    ? (email.attachments ?? []).filter((a) => a.cid === null)
    : [];
  const verified = expanded && isVerified(email);

  const headTop = (
    <div className={styles.headTop}>
      <span className={styles.sender}>{senderName(email)}</span>
      {expanded &&
        email.from?.[0]?.email !== undefined &&
        email.from[0].email !== senderName(email) && (
          <span className={styles.senderEmail}>{`<${email.from[0].email}>`}</span>
        )}
      {verified && (
        <span className={styles.verified} title={strings.senderVerifiedTitle}>
          <ShieldCheck className={styles.verifiedIcon} aria-hidden="true" />
          {strings.senderVerified}
        </span>
      )}
      {email.hasAttachment && <Paperclip className={styles.clip} aria-hidden="true" />}
      <span className={styles.date}>{formatDate(email.receivedAt)}</span>
    </div>
  );

  const noBody =
    textSplit === null && htmlSplit === null && attachments.length === 0;

  return (
    <article className={cx(styles.message, expanded && styles.expanded)}>
      {expanded ? (
        <div className={styles.head}>
          <Avatar name={senderName(email)} email={email.from?.[0]?.email} size="md" />
          <div className={styles.headText}>
            <button type="button" className={styles.headToggle} onClick={onToggle} aria-expanded>
              {headTop}
            </button>
            {/* Gmail "to me ▾" — a compact recipient line that expands to the full
                To / Cc / Bcc detail. */}
            <div className={styles.recipients}>
              <button
                type="button"
                className={styles.recipSummary}
                onClick={() => setRecipOpen((v) => !v)}
                aria-expanded={recipOpen}
              >
                {strings.toLabel} {recipientLine(email.to, me) || strings.recipientsNone}
                <ChevronDown
                  size={13}
                  className={cx(styles.recipCaret, recipOpen && styles.recipCaretOpen)}
                />
              </button>
              {recipOpen && (
                <div className={styles.recipDetail}>
                  <RecipientRow label={strings.toLabel} people={email.to} me={me} />
                  <RecipientRow label={strings.ccLabel} people={email.cc} me={me} />
                  <RecipientRow label={strings.bccLabel} people={email.bcc} me={me} />
                </div>
              )}
            </div>
          </div>
        </div>
      ) : (
        <button type="button" className={styles.head} onClick={onToggle} aria-expanded={false}>
          <Avatar name={senderName(email)} email={email.from?.[0]?.email} size="sm" />
          <span className={styles.collapsedSender}>{senderName(email)}</span>
          <span className={styles.collapsedPreview}>{email.preview}</span>
          {email.hasAttachment && <Paperclip className={styles.clip} aria-hidden="true" />}
          <span className={styles.date}>{formatDate(email.receivedAt)}</span>
        </button>
      )}

      {expanded && (
        <div className={styles.body} data-selectable>

          {email["alo:invitation"] != null && (
            <InvitationCard invitation={email["alo:invitation"]} blobId={email.blobId} />
          )}
          {textSplit !== null && <pre className={styles.text}>{textSplit.main}</pre>}
          {htmlSplit !== null && (
            <iframe
              className={styles.html}
              title={subjectOr(email)}
              sandbox=""
              srcDoc={sandboxedHtml(applyInlineImages(htmlSplit.main, inlineImages))}
            />
          )}

          {/* Gmail "···" — the quoted history below, collapsed by default. */}
          {(textSplit?.quoted != null || htmlSplit?.quoted != null) && (
            <div className={styles.quoted}>
              <button
                type="button"
                className={cx(styles.quotedToggle, quotedOpen && styles.quotedToggleOpen)}
                onClick={() => setQuotedOpen((v) => !v)}
                aria-label={strings.showQuoted}
                aria-expanded={quotedOpen}
              >
                <MoreHorizontal size={16} />
              </button>
              {quotedOpen && textSplit?.quoted != null && (
                <pre className={cx(styles.text, styles.quotedText)}>{textSplit.quoted}</pre>
              )}
              {quotedOpen && htmlSplit?.quoted != null && (
                <iframe
                  className={styles.html}
                  title={strings.showQuoted}
                  sandbox=""
                  srcDoc={sandboxedHtml(applyInlineImages(htmlSplit.quoted, inlineImages))}
                />
              )}
            </div>
          )}

          {noBody && <p className={styles.empty}>{email.preview}</p>}
          {attachments.length > 0 && (
            <div className={styles.attachments}>
              {attachments.map((a) => (
                <AttachmentChip key={a.blobId} attachment={a} />
              ))}
            </div>
          )}
        </div>
      )}
    </article>
  );
}
