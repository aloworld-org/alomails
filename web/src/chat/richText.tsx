// What a message body looks like once it is read rather than stored.
//
// Chat stores plain text and always will: the transcript is a record, and a
// record you can diff and search beats one carrying markup nobody can audit.
// So formatting lives here, at the reading end — the stored body is exactly
// what the person typed, and this decides how it appears.
//
// The vocabulary is the one people already type without being asked to:
// `code`, **bold**, _italic_, ~~struck~~, fenced blocks, $math$, bullets and
// numbers, and bare links. Nothing invented; nothing that needs a toolbar.
//
// Math and code reuse what mail and docs already use — KaTeX and Prism, via
// the authoring module (ADR 0015). One renderer across the workspace means a
// formula pasted from a document reads the same in a room.
import { Fragment } from "react";
import type { ReactNode } from "react";
import katex from "katex";

import { highlight } from "../authoring/prism";
import styles from "./richText.module.css";

/** A fenced code block, or display math, or a run of ordinary lines. */
type Block =
  | { kind: "code"; language: string; text: string }
  | { kind: "math"; text: string }
  | { kind: "quote"; lines: string[] }
  | { kind: "list"; ordered: boolean; items: string[] }
  | { kind: "text"; lines: string[] };

const FENCE = /^```(\w+)?\s*$/;
const DISPLAY_MATH = /^\$\$\s*$/;
const BULLET = /^\s*[-*]\s+(.*)$/;
const NUMBERED = /^\s*\d+[.)]\s+(.*)$/;
const QUOTE = /^\s*>\s?(.*)$/;

/** Split a body into blocks. Line-oriented, because that is how people type. */
function blocks(body: string): Block[] {
  const lines = body.split("\n");
  const out: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i] ?? "";

    const fence = FENCE.exec(line);
    if (fence !== null) {
      const text: string[] = [];
      i += 1;
      // An unclosed fence still renders as code to the end of the message —
      // someone mid-paste should see code, not the fence characters.
      while (i < lines.length && !FENCE.test(lines[i] ?? "")) {
        text.push(lines[i] ?? "");
        i += 1;
      }
      i += 1;
      out.push({
        kind: "code",
        language: fence[1] ?? "",
        text: text.join("\n"),
      });
      continue;
    }

    if (DISPLAY_MATH.test(line)) {
      const text: string[] = [];
      i += 1;
      while (i < lines.length && !DISPLAY_MATH.test(lines[i] ?? "")) {
        text.push(lines[i] ?? "");
        i += 1;
      }
      i += 1;
      out.push({ kind: "math", text: text.join("\n") });
      continue;
    }

    if (QUOTE.test(line)) {
      const quoted: string[] = [];
      while (i < lines.length && QUOTE.test(lines[i] ?? "")) {
        quoted.push(QUOTE.exec(lines[i] ?? "")?.[1] ?? "");
        i += 1;
      }
      out.push({ kind: "quote", lines: quoted });
      continue;
    }

    const isBullet = BULLET.test(line);
    const isNumbered = NUMBERED.test(line);
    if (isBullet || isNumbered) {
      const pattern = isBullet ? BULLET : NUMBERED;
      const items: string[] = [];
      while (i < lines.length && pattern.test(lines[i] ?? "")) {
        items.push(pattern.exec(lines[i] ?? "")?.[1] ?? "");
        i += 1;
      }
      out.push({ kind: "list", ordered: isNumbered, items });
      continue;
    }

    const text: string[] = [];
    while (
      i < lines.length &&
      !FENCE.test(lines[i] ?? "") &&
      !DISPLAY_MATH.test(lines[i] ?? "") &&
      !QUOTE.test(lines[i] ?? "") &&
      !BULLET.test(lines[i] ?? "") &&
      !NUMBERED.test(lines[i] ?? "")
    ) {
      text.push(lines[i] ?? "");
      i += 1;
    }
    out.push({ kind: "text", lines: text });
  }
  return out;
}

/** Inline spans, innermost-first so `code` wins over everything inside it. */
const INLINE = [
  { kind: "code", re: /`([^`\n]+)`/ },
  { kind: "math", re: /\$([^$\n]+)\$/ },
  { kind: "bold", re: /\*\*([^*\n]+)\*\*/ },
  { kind: "strike", re: /~~([^~\n]+)~~/ },
  { kind: "italic", re: /(?:^|[\s(])_([^_\n]+)_(?=[\s.,!?)]|$)/ },
  { kind: "link", re: /(https?:\/\/[^\s<>"]+)/ },
] as const;

/** Render one line's inline formatting, marking `@handles` as it goes. */
function inline(text: string, mark: (t: string) => ReactNode[]): ReactNode[] {
  // Find whichever span starts earliest; ties go to the earlier rule, which is
  // why `code` is first — backticks must win over anything inside them.
  let best: { kind: string; at: number; len: number; body: string } | null =
    null;
  for (const { kind, re } of INLINE) {
    const m = re.exec(text);
    if (m === null) continue;
    // The italic rule matches a leading space or bracket so that `snake_case`
    // is left alone; that character is not part of the span, so skip past it.
    const lead = kind === "italic" ? m[0].indexOf("_") : 0;
    const at = m.index + lead;
    if (best === null || at < best.at) {
      best = { kind, at, len: m[0].length - lead, body: m[1] ?? "" };
    }
  }
  if (best === null) return mark(text);

  const before = text.slice(0, best.at);
  const after = text.slice(best.at + best.len);
  const middle = ((): ReactNode => {
    switch (best.kind) {
      case "code":
        return <code className={styles.code}>{best.body}</code>;
      case "math":
        return <Math latex={best.body} display={false} />;
      case "bold":
        return <strong>{inline(best.body, mark)}</strong>;
      case "italic":
        return <em>{inline(best.body, mark)}</em>;
      case "strike":
        return <s>{inline(best.body, mark)}</s>;
      case "link":
        return (
          // `noreferrer` as well as `noopener`: a workplace link should not
          // hand the destination this workspace's URL.
          <a
            className={styles.link}
            href={best.body}
            target="_blank"
            rel="noopener noreferrer"
          >
            {best.body}
          </a>
        );
      default:
        return best.body;
    }
  })();

  return [
    ...mark(before),
    <Fragment key={`${best.kind}-${best.at}`}>{middle}</Fragment>,
    ...inline(after, mark),
  ];
}

/** KaTeX, rendered to a string we own — never user HTML. */
function Math({ latex, display }: { latex: string; display: boolean }) {
  let html: string;
  try {
    html = katex.renderToString(latex, {
      displayMode: display,
      throwOnError: false,
      output: "html",
    });
  } catch {
    // Unparseable maths stays as the text the person typed. Showing the source
    // is more useful than an error nobody can act on.
    return (
      <code className={styles.code}>
        {display ? `$$${latex}$$` : `$${latex}$`}
      </code>
    );
  }
  return (
    <span
      className={display ? styles.mathBlock : styles.mathInline}
      // Safe: this is KaTeX's own output from the LaTeX above, not anything
      // the sender supplied as markup.
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

/** A fenced block, highlighted by the same Prism setup mail and docs use. */
function Code({ language, text }: { language: string; text: string }) {
  const html = highlight(text, language);
  return (
    <pre className={styles.pre}>
      <code
        className={styles.preCode}
        // Safe: Prism's own tokenisation of the text above.
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </pre>
  );
}

/**
 * Render a stored message body for reading.
 *
 * `mark` handles `@handles`, so mention highlighting keeps working inside
 * formatted text rather than being lost the moment someone types a bullet.
 */
export function renderBody(
  body: string,
  mark: (text: string) => ReactNode[],
): ReactNode {
  return blocks(body).map((block, i) => {
    switch (block.kind) {
      case "code":
        return <Code key={i} language={block.language} text={block.text} />;
      case "math":
        return <Math key={i} latex={block.text} display />;
      case "quote":
        return (
          <blockquote key={i} className={styles.quote}>
            {block.lines.map((line, j) => (
              <Fragment key={j}>
                {inline(line, mark)}
                {j < block.lines.length - 1 && <br />}
              </Fragment>
            ))}
          </blockquote>
        );
      case "list":
        return block.ordered ? (
          <ol key={i} className={styles.list}>
            {block.items.map((item, j) => (
              <li key={j}>{inline(item, mark)}</li>
            ))}
          </ol>
        ) : (
          <ul key={i} className={styles.list}>
            {block.items.map((item, j) => (
              <li key={j}>{inline(item, mark)}</li>
            ))}
          </ul>
        );
      default:
        return (
          <Fragment key={i}>
            {block.lines.map((line, j) => (
              <Fragment key={j}>
                {inline(line, mark)}
                {j < block.lines.length - 1 && <br />}
              </Fragment>
            ))}
          </Fragment>
        );
    }
  });
}
