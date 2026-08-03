// Pure helpers to pull display text out of a JMAP Email. Kept separate from
// the component so body selection is testable and the rendering component
// stays about safety and layout.
import type { EmailBodyPart, EmailFull } from "../jmap";

function join(parts: EmailBodyPart[], values: EmailFull["bodyValues"]): string | null {
  const chunks: string[] = [];
  for (const part of parts) {
    if (part.partId === null) continue;
    const v = values[part.partId];
    if (v !== undefined) chunks.push(v.value);
  }
  return chunks.length > 0 ? chunks.join("\n") : null;
}

/** The plain-text body, if the message has one. */
export function textContent(email: EmailFull): string | null {
  return join(email.textBody, email.bodyValues);
}

/** The HTML body, if the message has one. */
export function htmlContent(email: EmailFull): string | null {
  return join(email.htmlBody, email.bodyValues);
}

/** A body split into the new message and the quoted history below it (Gmail's
 * "···" collapse). `quoted` is null when there's nothing to collapse. */
export interface SplitBody {
  main: string;
  quoted: string | null;
}

// Attribution lines that begin a quoted reply, e.g. "On Tue, Jul 29… wrote:".
const TEXT_QUOTE_MARKERS = [
  /^On .+ wrote:\s*$/m,
  /^-{2,}\s*Original Message\s*-{2,}\s*$/im,
  /^_{5,}\s*$/m,
  /^From: .+$/m,
];

/** Split a plain-text body at the first quoted-reply boundary. */
export function splitQuotedText(text: string): SplitBody {
  let cut = -1;
  for (const re of TEXT_QUOTE_MARKERS) {
    const m = re.exec(text);
    if (m !== null && (cut === -1 || m.index < cut)) cut = m.index;
  }
  // Also treat a run of leading-">" lines as the start of the quote.
  const lines = text.split("\n");
  let quoteLine = -1;
  let offset = 0;
  for (const line of lines) {
    if (/^\s*>/.test(line)) {
      quoteLine = offset;
      break;
    }
    offset += line.length + 1;
  }
  if (quoteLine !== -1 && (cut === -1 || quoteLine < cut)) cut = quoteLine;

  if (cut <= 0) return { main: text, quoted: null };
  const main = text.slice(0, cut).replace(/\s+$/, "");
  const quoted = text.slice(cut).replace(/^\s+/, "");
  return quoted.length > 0 ? { main, quoted } : { main: text, quoted: null };
}

// HTML wrappers that mail clients use for the quoted history.
const HTML_QUOTE_MARKERS = [
  /<blockquote\b/i,
  /<div[^>]+class="[^"]*gmail_quote[^"]*"/i,
  /<div[^>]+class="[^"]*moz-cite-prefix[^"]*"/i,
  /<div[^>]+id="[^"]*(?:appendonsend|divRplyFwdMsg)[^"]*"/i,
];

/** Split an HTML body at the first quoted-reply wrapper. */
export function splitQuotedHtml(html: string): SplitBody {
  let cut = -1;
  for (const re of HTML_QUOTE_MARKERS) {
    const m = re.exec(html);
    if (m !== null && (cut === -1 || m.index < cut)) cut = m.index;
  }
  if (cut <= 0) return { main: html, quoted: null };
  return { main: html.slice(0, cut), quoted: html.slice(cut) };
}

/** A rough plain-text rendering of a message body for feeding to the summarizer
 * (never for display): prefer the text part, else strip tags off the HTML. */
function plainBody(email: EmailFull): string {
  const text = textContent(email);
  if (text !== null) return text;
  const html = htmlContent(email);
  if (html === null) return email.preview;
  return html
    .replace(/<(script|style)[\s\S]*?<\/\1>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/gi, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/** Concatenate a thread into a labelled plain-text digest for summarization.
 * Order is the display order (oldest first); each turn is prefixed with its
 * sender so the model can attribute statements. */
export function threadDigest(messages: EmailFull[]): string {
  return messages
    .map((m) => {
      const who = m.from?.[0]?.name ?? m.from?.[0]?.email ?? "Unknown";
      return `${who}:\n${plainBody(m)}`;
    })
    .join("\n\n---\n\n")
    .trim();
}

/** Wrap untrusted HTML with a strict CSP for a sandboxed iframe: no scripts,
 * no remote anything (blocks tracking pixels — privacy is the brand); inline
 * styles and data: images (inline attachments) are allowed. */
export function sandboxedHtml(html: string): string {
  const csp =
    "default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:; media-src data:";
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${csp}"><style>body{font-family:Inter,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;color:#211d18;margin:0;padding:16px;line-height:1.6}img{max-width:100%}</style></head><body>${html}</body></html>`;
}
