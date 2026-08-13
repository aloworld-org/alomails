// Typing on the page, turned into the one change shape the server already
// speaks (ADR 0042).
//
// The preview document marks every element that holds exactly one typed
// string with `data-alo-text="<section index><JSON pointer>"` and reports a
// finished edit to this app by `postMessage`. Everything here is the reply to
// that message: prove the sender, prove the coordinate against the sections
// this app is actually holding, and build a `rewrite_copy` operation — the
// same operation an AI proposal contains, applied through the same guarded
// door. There is no second edit path, so there is no second thing to get
// wrong. The undo history that covers it is shared with every other gesture
// on the page and lives in `editHistory.ts`.
//
// Nothing here trusts the frame. Its document has no origin, it is built from
// tenant content, and a message is a string somebody typed: the key is only
// ever used to look a value up in state this app already had, never as an
// instruction. A key naming a section that has moved, a pointer that is not a
// string leaf, or text past the cap yields `null` and the editor says so.
import type { Section } from "./sections";
import type { SiteEditEnvelope, SiteEditOperation, SiteEditTarget } from "./types";

/** The envelope version this build writes; the server refuses any other. */
const EDIT_SCHEMA_VERSION = 1;

/** Longest text one inline edit may carry. The section schema has its own,
 *  per-property limits and enforces them at the door — this is the cheap
 *  refusal before a pointless round trip. */
export const MAX_INLINE_TEXT = 5000;

/** What the preview frame reports when a person finishes typing. */
export interface InlineTextEdit {
  /** `"<section index><JSON pointer>"`, exactly as the renderer wrote it. */
  key: string;
  /** The finished text, as plain characters. */
  text: string;
}

/**
 * Reads the message the preview frame posted, or `null` for anything else.
 *
 * `event.origin` is `"null"` for a sandboxed `srcdoc` document and therefore
 * proves nothing, so the caller proves the *sender* instead by passing the
 * frame's own window; that is the check this app can actually rely on.
 */
export function readTextEditMessage(
  data: unknown,
  fromPreview: boolean,
): InlineTextEdit | null {
  if (!fromPreview || typeof data !== "object" || data === null) return null;
  const message = data as Record<string, unknown>;
  if (message["alo"] !== "site-text-edit") return null;
  const key = message["key"];
  const text = message["text"];
  if (typeof key !== "string" || typeof text !== "string") return null;
  if (key.length === 0 || text.length > MAX_INLINE_TEXT) return null;
  return { key, text };
}

/** Splits `"2/items/0/title"` into the section index and the JSON pointer
 *  inside it. Anything that is not a plain index followed by a pointer is
 *  refused — the coordinate has one spelling. */
export function splitTextKey(
  key: string,
): { index: number; pointer: string } | null {
  const slash = key.indexOf("/");
  if (slash <= 0) return null;
  const index = key.slice(0, slash);
  if (!/^\d+$/.test(index)) return null;
  return { index: Number(index), pointer: key.slice(slash) };
}

/** Resolves an RFC 6901 pointer against a section and answers the string it
 *  addresses — `null` when the pointer misses, or when what it names is not a
 *  string. `rewrite_copy` rewrites existing text and nothing else, and this is
 *  the same rule read on this side before the round trip. */
export function pointerText(section: Section, pointer: string): string | null {
  let cursor: unknown = section;
  for (const raw of pointer.split("/").slice(1)) {
    const token = raw.replaceAll("~1", "/").replaceAll("~0", "~");
    if (Array.isArray(cursor)) {
      if (!/^\d+$/.test(token)) return null;
      cursor = cursor[Number(token)];
    } else if (typeof cursor === "object" && cursor !== null) {
      cursor = (cursor as Record<string, unknown>)[token];
    } else {
      return null;
    }
    if (cursor === undefined) return null;
  }
  return typeof cursor === "string" ? cursor : null;
}

/** The section a key names, when the page still has it. */
export function keyTarget(
  sections: Section[],
  key: string,
): { target: SiteEditTarget; pointer: string; current: string } | null {
  const split = splitTextKey(key);
  if (split === null) return null;
  const section = sections[split.index];
  if (section === undefined) return null;
  const current = pointerText(section, split.pointer);
  if (current === null) return null;
  return {
    target: { index: split.index, type: section.type },
    pointer: split.pointer,
    current,
  };
}

/**
 * The change a person typing on the page is asking for, as the operation the
 * model would have proposed for the same change — same `op`, same target,
 * same pointer, same text.
 *
 * `null` means the page moved under the gesture (the section is gone, or is a
 * different type, or the pointer no longer names text): the edit is refused
 * here rather than aimed at whatever now sits at that index.
 */
export function textEditOperation(
  sections: Section[],
  key: string,
  text: string,
): SiteEditOperation | null {
  const found = keyTarget(sections, key);
  if (found === null || text.length > MAX_INLINE_TEXT) return null;
  return {
    op: "rewrite_copy",
    target: found.target,
    pointer: found.pointer,
    text,
  };
}

/** One operation, wrapped in the envelope the edit door takes. */
export function textEditEnvelope(operation: SiteEditOperation): SiteEditEnvelope {
  return { schema_version: EDIT_SCHEMA_VERSION, operations: [operation] };
}
