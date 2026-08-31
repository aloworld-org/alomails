import { useState } from "react";

import { strings } from "../i18n/strings";
import type { MessageSource } from "./types";

/** What an agent's answer was grounded in, under the answer.
 *
 *  An agent may answer only from a numbered list it was handed, and cites each
 *  claim by its number — "Ben owns the rollout [2]". Until this existed the
 *  room showed the number and never the list, so a reader met a footnote
 *  marker with no footnote: it invites trust ("it cited something") while
 *  withholding the one thing a citation is for.
 *
 *  Collapsed by default. The answer is the thing being read; the sources are
 *  what you open when you want to check it, the way a footnote works on paper.
 */
export function MessageSources({ sources }: { sources: MessageSource[] }) {
  const [open, setOpen] = useState(false);
  if (sources.length === 0) return null;
  const summary =
    sources.length === 1
      ? strings.chatSourceOne
      : strings.chatSourceCount.replace("{count}", String(sources.length));
  return (
    <div className="mt-1">
      <button
        type="button"
        className="rounded-sm text-xs text-tertiary underline decoration-dotted underline-offset-2 hover:text-secondary focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        aria-expanded={open}
        onClick={() => setOpen((was) => !was)}
      >
        {summary}
      </button>
      {open && (
        <ol className="mt-1 flex list-none flex-col gap-1 p-0">
          {sources.map((source) => (
            <li key={source.n} className="flex gap-2 text-xs text-secondary">
              {/* The number is the whole point: it is what the answer's
                  bracket points at, so it leads and is not decoration. */}
              <span className="shrink-0 tabular-nums font-semibold text-tertiary">
                [{source.n}]
              </span>
              <span className="min-w-0">
                <span className="text-tertiary">{kindLabel(source.kind)}</span>
                {source.title !== "" && (
                  <>
                    {" — "}
                    <span className="text-secondary">{source.title}</span>
                  </>
                )}
              </span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

/** A source's kind, said the way a person would say it.
 *
 *  The server sends the word the grounding used. The four that read badly as
 *  bare labels are translated; anything else — a reading tool's own name, and
 *  new ones arrive whenever a module gains a verb — is shown as it came, which
 *  is better than an empty space or a guess. */
function kindLabel(kind: string): string {
  switch (kind) {
    case "message":
      return strings.chatSourceEmail;
    case "chat":
      return strings.chatSourceChat;
    case "event":
      return strings.chatSourceEvent;
    case "remembered":
      return strings.chatSourceRemembered;
    default:
      return kind;
  }
}
