// A tokenized recipient field: existing recipients render as removable chips
// (avatar + name/email), and typing an address then Enter / comma / semicolon /
// Tab — or blurring — commits it as a new chip. Backspace on an empty input
// removes the last chip. As you type, recent correspondents matching the text
// drop down for one-click selection. Used for To / Cc / Bcc in compose.
import { useMemo, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";

import { strings } from "../../i18n";
import { Avatar, Chip } from "../../ds";
import type { EmailAddress } from "../../jmap";
import { senderName } from "../format";
import styles from "./RecipientInput.module.css";

/** How many autocomplete suggestions to show at once. */
const MAX_SUGGESTIONS = 6;

interface RecipientInputProps {
  label: string;
  value: EmailAddress[];
  onChange: (next: EmailAddress[]) => void;
  autoFocus?: boolean;
  /** Recent correspondents to offer as autocomplete suggestions. */
  suggestions?: EmailAddress[];
  /** Extra controls rendered at the right of the row (e.g. Cc/Bcc toggles). */
  trailing?: ReactNode;
}

/** Split raw text into candidate addresses and keep the ones with an "@". */
function parseAddresses(text: string): EmailAddress[] {
  return text
    .split(/[,;\s]+/)
    .map((s) => s.trim())
    .filter((s) => s.includes("@"))
    .map((email) => ({ name: null, email }));
}

function chipLabel(a: EmailAddress): string {
  return a.name !== null && a.name.trim().length > 0 ? a.name : a.email;
}

export function RecipientInput({
  label,
  value,
  onChange,
  autoFocus,
  suggestions,
  trailing,
}: RecipientInputProps) {
  const [draft, setDraft] = useState("");
  const [focused, setFocused] = useState(false);
  const [active, setActive] = useState(0);

  // Suggestions matching the current text, excluding already-added recipients.
  const matches = useMemo(() => {
    const q = draft.trim().toLowerCase();
    if (q.length === 0 || suggestions === undefined) return [];
    const chosen = new Set(value.map((a) => a.email.toLowerCase()));
    return suggestions
      .filter((c) => !chosen.has(c.email.toLowerCase()))
      .filter(
        (c) =>
          c.email.toLowerCase().includes(q) ||
          (c.name !== null && c.name.toLowerCase().includes(q)),
      )
      .slice(0, MAX_SUGGESTIONS);
  }, [draft, suggestions, value]);

  const showList = focused && matches.length > 0;

  function commit(text: string): boolean {
    const parsed = parseAddresses(text);
    if (parsed.length === 0) return false;
    const seen = new Set(value.map((a) => a.email.toLowerCase()));
    const added = parsed.filter((a) => {
      const key = a.email.toLowerCase();
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
    if (added.length > 0) onChange([...value, ...added]);
    return true;
  }

  function choose(contact: EmailAddress) {
    if (!value.some((a) => a.email.toLowerCase() === contact.email.toLowerCase())) {
      onChange([...value, contact]);
    }
    setDraft("");
    setActive(0);
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (showList && e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => (i + 1) % matches.length);
    } else if (showList && e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => (i - 1 + matches.length) % matches.length);
    } else if (e.key === "Enter") {
      // A highlighted suggestion wins; otherwise commit the typed text.
      const picked = showList ? matches[active] : undefined;
      if (picked !== undefined) {
        e.preventDefault();
        choose(picked);
      } else if (draft.trim().length > 0) {
        e.preventDefault();
        if (commit(draft)) setDraft("");
      }
    } else if ((e.key === "," || e.key === ";" || e.key === "Tab") && draft.trim().length > 0) {
      e.preventDefault();
      if (commit(draft)) setDraft("");
    } else if (e.key === "Escape" && showList) {
      e.preventDefault();
      setFocused(false);
    } else if (e.key === "Backspace" && draft.length === 0 && value.length > 0) {
      onChange(value.slice(0, -1));
    }
  }

  function remove(index: number) {
    onChange(value.filter((_, i) => i !== index));
  }

  return (
    <div className={styles.row}>
      <span className={styles.label}>{label}</span>
      <div className={styles.wrap}>
        <div className={styles.tokens}>
          {value.map((a, i) => (
            <Chip
              key={`${a.email}-${i}`}
              className={styles.recipient}
              onRemove={() => remove(i)}
              removeLabel={strings.removeRecipient(chipLabel(a))}
            >
              <Avatar name={senderName({ from: [a] })} email={a.email} size="sm" />
              <span className={styles.recipientName}>{chipLabel(a)}</span>
            </Chip>
          ))}
          <input
            className={styles.entry}
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value);
              setActive(0);
            }}
            onKeyDown={onKeyDown}
            onFocus={() => setFocused(true)}
            onBlur={() => {
              setFocused(false);
              if (commit(draft)) setDraft("");
            }}
            autoFocus={autoFocus}
            aria-label={label}
            aria-autocomplete="list"
            aria-expanded={showList}
          />
        </div>
        {showList && (
          <ul className={styles.suggestions} role="listbox" aria-label={strings.contactSuggestions}>
            {matches.map((c, i) => (
              <li key={c.email} role="option" aria-selected={i === active}>
                <button
                  type="button"
                  className={i === active ? styles.suggestActive : styles.suggest}
                  // mousedown (not click) so the input doesn't blur-commit first.
                  onMouseDown={(e) => {
                    e.preventDefault();
                    choose(c);
                  }}
                  onMouseEnter={() => setActive(i)}
                >
                  <Avatar name={senderName({ from: [c] })} email={c.email} size="sm" />
                  <span className={styles.suggestText}>
                    {c.name !== null && c.name.trim().length > 0 && (
                      <span className={styles.suggestName}>{c.name}</span>
                    )}
                    <span className={styles.suggestEmail}>{c.email}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      {trailing !== undefined && <div className={styles.trailing}>{trailing}</div>}
    </div>
  );
}
