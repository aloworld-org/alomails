// Resizing a section within its own constraints (ADR 0042, S3.01c), turned
// into the one change shape the server already speaks.
//
// The vocabulary is not written here. The store declares, per section type,
// which properties can be resized, the JSON pointer each lives at and the
// complete ordered list of values it may take; the editor reads that
// declaration from `GET /sites/config` and offers exactly what it names. So a
// value this app has never heard of cannot be produced by it, and a value the
// server would refuse cannot be offered by it — there is one vocabulary, in
// the crate that validates writes against it.
//
// The gesture on the page carries even less: `{alo:"site-section-layout",
// index, step}` where `step` is -1 or +1. A direction, never a number and
// never a value — the frame cannot name a size, so no gesture inside it can
// produce free positioning even if the document it runs in is hostile. This
// app resolves the direction against the declaration and the section it is
// holding, and applies the result as a `set_prop` operation through the same
// guarded edit door an approved AI proposal goes through.
import type { Section } from "./sections";
import type { SiteEditOperation } from "./types";

/** One resizable property of one section type, exactly as `/sites/config`
 *  declares it. */
export interface SectionLayoutControl {
  /** Stable name (`split`, `columns`, `shape`). */
  key: string;
  /** RFC 6901 pointer to the property inside the section. */
  pointer: string;
  /** Every value it offers, in order — narrowest first. */
  values: string[];
  /** What an absent value renders as. */
  default: string;
}

/** The declaration keyed by section type; types absent from it cannot be
 *  resized at all. */
export type SectionLayouts = Record<string, SectionLayoutControl[]>;

/** What the preview frame reports when a section is resized on the page: a
 *  direction, on the section at `index`. */
export interface SectionLayoutStep {
  index: number;
  /** -1 for the previous declared value, +1 for the next. */
  step: -1 | 1;
}

/** Reads the message the preview frame posted, or `null` for anything else.
 *
 *  `event.origin` is `"null"` for a sandboxed `srcdoc` document and proves
 *  nothing, so the caller proves the *sender* by passing the frame's own
 *  window. A step that is not exactly one place in one direction is not a
 *  gesture this editor has. */
export function readLayoutStepMessage(
  data: unknown,
  fromPreview: boolean,
): SectionLayoutStep | null {
  if (!fromPreview || typeof data !== "object" || data === null) return null;
  const message = data as Record<string, unknown>;
  if (message["alo"] !== "site-section-layout") return null;
  const index = message["index"];
  const step = message["step"];
  if (!Number.isInteger(index) || (index as number) < 0) return null;
  if (step !== -1 && step !== 1) return null;
  return { index: index as number, step };
}

/** Reads the declaration off a `/sites/config` body, dropping anything that
 *  is not shaped like a control. A malformed catalog costs the resize
 *  affordance, never the editor. */
export function readSectionLayouts(value: unknown): SectionLayouts {
  if (typeof value !== "object" || value === null) return {};
  const layouts: SectionLayouts = {};
  for (const [kind, raw] of Object.entries(value as Record<string, unknown>)) {
    if (!Array.isArray(raw)) continue;
    const controls = raw.filter(isControl);
    if (controls.length > 0) layouts[kind] = controls;
  }
  return layouts;
}

function isControl(value: unknown): value is SectionLayoutControl {
  if (typeof value !== "object" || value === null) return false;
  const control = value as Record<string, unknown>;
  return (
    typeof control["key"] === "string" &&
    typeof control["pointer"] === "string" &&
    control["pointer"].startsWith("/") &&
    typeof control["default"] === "string" &&
    Array.isArray(control["values"]) &&
    control["values"].length > 0 &&
    control["values"].every((v) => typeof v === "string")
  );
}

/** The controls a section actually offers: declared for its type, and with
 *  the property's parent present. A hero with no image has no shape to
 *  choose, and offering one would offer a change the edit door refuses. */
export function controlsFor(
  layouts: SectionLayouts,
  section: Section | undefined,
): SectionLayoutControl[] {
  if (section === undefined) return [];
  return (layouts[section.type] ?? []).filter((control) =>
    hasParent(section, control.pointer),
  );
}

/** Resolves a pointer against a section, or `undefined` when it misses. */
function pointerValue(section: Section, pointer: string): unknown {
  let cursor: unknown = section;
  for (const raw of pointer.split("/").slice(1)) {
    const token = raw.replaceAll("~1", "/").replaceAll("~0", "~");
    if (typeof cursor !== "object" || cursor === null) return undefined;
    cursor = (cursor as Record<string, unknown>)[token];
    if (cursor === undefined) return undefined;
  }
  return cursor;
}

/** Whether the object a pointer's last token lives in exists. */
function hasParent(section: Section, pointer: string): boolean {
  const parent = pointer.slice(0, pointer.lastIndexOf("/"));
  if (parent === "") return true;
  const value = pointerValue(section, parent);
  return typeof value === "object" && value !== null;
}

/** The value a control is currently on — what is stored, or the declared
 *  default when nothing has been chosen. A stored value the declaration does
 *  not offer (an older build's, a newer server's) reads as the default rather
 *  than as a fourth state the buttons cannot show. */
export function currentValue(
  section: Section,
  control: SectionLayoutControl,
): string {
  const stored = pointerValue(section, control.pointer);
  return typeof stored === "string" && control.values.includes(stored)
    ? stored
    : control.default;
}

/** One place along the control's own list, or `null` at either end. Stepping
 *  never wraps: a gesture that silently jumps from the widest to the narrowest
 *  is a gesture nobody meant to make. */
export function steppedValue(
  control: SectionLayoutControl,
  current: string,
  step: -1 | 1,
): string | null {
  const at = control.values.indexOf(current);
  if (at < 0) return null;
  return control.values[at + step] ?? null;
}

/**
 * The change a resize is asking for, as the operation a model would have
 * proposed for the same change — same `op`, same target, same pointer.
 *
 * `null` means the request does not resolve against this page: no such
 * section, no such control on its type, or a value the declaration does not
 * offer. Nothing is guessed and nothing is clamped; the caller shows the
 * page as it is instead.
 */
export function layoutOperation(
  sections: Section[],
  layouts: SectionLayouts,
  index: number,
  key: string,
  value: string,
): SiteEditOperation | null {
  const section = sections[index];
  if (section === undefined) return null;
  const control = controlsFor(layouts, section).find((c) => c.key === key);
  if (control === undefined || !control.values.includes(value)) return null;
  return {
    op: "set_prop",
    target: { index, type: section.type },
    pointer: control.pointer,
    value,
  };
}
