// The editing machinery a billing document shares with every other billing
// document: loading one, holding the form, and the autosave loop that keeps
// the server's figures on screen.
//
// It exists because an invoice draft and a quote draft are the same object
// with different words on it — same header, same line grid, same "the totals
// are whatever the server last said" rule — and two copies of an autosave loop
// is two places for a document to be saved wrongly.
//
// Three rules it encodes, all of them from `docs/design/billing.md`:
//
// - **Only the header fields that actually changed are sent.** Restating the
//   customer would send the document back through the store's customer check
//   on every keystroke, and a draft raised for a customer archived afterwards
//   would then refuse to have its lines edited at all — a dead end with no way
//   out but deleting it.
// - **One request is in flight at a time.** An edit that lands mid-save bumps
//   a counter and the loop goes round again with the newest form, rather than
//   racing it; a save that finishes into a changed form never reports "saved".
// - **A row that is not yet a line holds the save.** The API replaces the
//   whole line set in one write, so a set that quietly left the offending row
//   out would *delete* the line it stands for.
import { useCallback, useEffect, useRef, useState } from "react";

import { BillingError, billingMessage } from "./api";
import { rowFromLine, rowsDraft } from "./lineRows";
import type { LineRow } from "./lineRows";
import { strings } from "../i18n";
import type { DocumentLine, DocumentTotals, LineDraft } from "./types";

/** How long typing has to stop before the draft saves itself. Long enough not
 *  to write a document per keystroke, short enough that the totals feel like
 *  they belong to what is on screen. */
export const AUTOSAVE_MS = 700;

/** The header fields a person edits on any billing document.
 *
 *  The currency, the payment term and a quote's validity are deliberately not
 *  among them: each is snapshotted from the customer when the document is
 *  raised, and what a document is denominated in — or how long an offer stands
 *  — is not a text box (`docs/design/billing.md`). */
export interface DocumentHeader {
  customerId: string;
  reference: string;
  note: string;
}

/** What a save sends: the header fields that changed, and always the whole
 *  line set, because the API replaces it in one write. */
export type DocumentPatch = Partial<DocumentHeader> & { lines: LineDraft[] };

/** The shape both an invoice and a quote have, and all this module needs of
 *  either: who it is for, what it is worth, and what is on it. */
export interface StoredDocument extends DocumentHeader {
  id: string;
  currency: string;
  /** The legal number, or `null` while the document has not consumed one. */
  number: string | null;
  lines: DocumentLine[];
  totals: DocumentTotals;
}

/** Where the draft stands against the server. */
export type SaveState = "saved" | "pending" | "saving" | "failed";

/**
 * How a particular kind of document is loaded and saved.
 *
 * `load` answers the document **and** whatever else that document's screen
 * shows about it — the credit notes raised against an invoice, the invoice an
 * accepted quote produced — so opening one record is one request.
 */
export interface DraftPorts<T extends StoredDocument, A> {
  /** The id being edited, or `undefined` on the "new document" screen. */
  id: string | undefined;
  load: (id: string) => Promise<{ document: T; aside: A }>;
  create: (draft: Partial<DocumentHeader> & { lines?: LineDraft[] }) => Promise<T>;
  save: (id: string, patch: DocumentPatch) => Promise<T>;
  /** Whether the stored document still takes edits. False freezes the form. */
  editable: (document: T) => boolean;
}

/** What the editor screen gets back. */
export interface DocumentDraft<T extends StoredDocument, A> {
  document: T | null;
  aside: A | null;
  header: DocumentHeader;
  rows: LineRow[];
  loading: boolean;
  /** The id was not found — a stale link, not a failure to show as an error. */
  missing: boolean;
  error: string | null;
  saveState: SaveState;
  /** The document exists and is frozen: the form renders read-only. */
  readOnly: boolean;
  /** True while the create request for a brand-new document is in flight. */
  creating: boolean;
  edit: (next: Partial<{ header: DocumentHeader; rows: LineRow[] }>) => void;
  /** Saves now, rather than waiting for the debounce (the retry action). */
  saveNow: () => void;
  /** Raises the document this screen was opened to write; answers it so the
   *  caller can navigate to the id the server gave it. */
  create: () => Promise<T | null>;
  /** Takes a document the server just answered with — after a lifecycle
   *  transition — as the form's new starting position. */
  adopt: (document: T) => void;
  /** Reports a failure from an action this hook does not own. */
  fail: (message: string) => void;
  /** A fresh row's key, from a counter that outlives a render. */
  nextKey: () => string;
}

/** The header fields of `edited` that differ from the stored document. */
function changedHeader(edited: DocumentHeader, base: DocumentHeader): Partial<DocumentHeader> {
  const patch: Partial<DocumentHeader> = {};
  if (edited.customerId !== base.customerId) patch.customerId = edited.customerId;
  if (edited.reference !== base.reference) patch.reference = edited.reference;
  if (edited.note !== base.note) patch.note = edited.note;
  return patch;
}

/**
 * The form behind one billing document: it loads the record, holds what is
 * typed, and saves it back a moment after typing stops.
 *
 * The document it answers with is always the server's own — every figure the
 * screen renders comes from a response, never from arithmetic here.
 */
export function useDocumentDraft<T extends StoredDocument, A>(
  ports: DraftPorts<T, A>,
): DocumentDraft<T, A> {
  const { id, load, create: createPort, save: savePort, editable } = ports;

  const [document, setDocument] = useState<T | null>(null);
  const [aside, setAside] = useState<A | null>(null);
  const [header, setHeader] = useState<DocumentHeader>({
    customerId: "",
    reference: "",
    note: "",
  });
  const [rows, setRows] = useState<LineRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [missing, setMissing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<SaveState>("saved");
  const [creating, setCreating] = useState(false);

  // What the autosave loop reads. State would be a render behind it, and a
  // save that sent a stale line set would silently undo a keystroke.
  const editRef = useRef<{ header: DocumentHeader; rows: LineRow[] }>({ header, rows });
  /** The document as the server last stored it — what a save is diffed
   *  against, kept in a ref for the same reason the edits are. */
  const savedRef = useRef<T | null>(null);
  const savingRef = useRef(false);
  /** Bumped on every edit, so a save that finishes into a changed form knows
   *  to go round again instead of reporting "saved". */
  const editSeq = useRef(0);
  /** Row identity for rows that are not stored lines yet. */
  const keySeq = useRef(0);
  const nextKey = useCallback(() => {
    keySeq.current += 1;
    return `new-${keySeq.current}`;
  }, []);

  const readOnly = document !== null && !editable(document);

  const adopt = useCallback((stored: T) => {
    savedRef.current = stored;
    setDocument(stored);
    const next = {
      header: {
        customerId: stored.customerId,
        reference: stored.reference,
        note: stored.note,
      },
      rows: stored.lines.map(rowFromLine),
    };
    editRef.current = next;
    setHeader(next.header);
    setRows(next.rows);
    setSaveState("saved");
  }, []);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        if (id !== undefined) {
          const loaded = await load(id);
          if (!live) return;
          adopt(loaded.document);
          setAside(loaded.aside);
        }
        if (live) setError(null);
      } catch (err) {
        if (!live) return;
        setMissing(err instanceof BillingError && err.status === 404);
        setError(billingMessage(err, strings.billingLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [id, load, adopt]);

  const edit = useCallback((next: Partial<{ header: DocumentHeader; rows: LineRow[] }>) => {
    editRef.current = { ...editRef.current, ...next };
    if (next.header !== undefined) setHeader(next.header);
    if (next.rows !== undefined) setRows(next.rows);
    editSeq.current += 1;
    setSaveState("pending");
  }, []);

  /** The body a save would send, or `null` while a row is not yet a line (or
   *  no customer has been chosen, which means there is no document yet). */
  const patchOf = useCallback(
    (edited: { header: DocumentHeader; rows: LineRow[] }, base: T): DocumentPatch | null => {
      const lines = rowsDraft(edited.rows);
      if (lines === null || edited.header.customerId === "") return null;
      return { ...changedHeader(edited.header, base), lines };
    },
    [],
  );

  const save = useCallback(async () => {
    if (savingRef.current || id === undefined) return;
    const base = savedRef.current;
    if (base === null) return;
    savingRef.current = true;
    try {
      for (;;) {
        const seq = editSeq.current;
        const patch = patchOf(editRef.current, savedRef.current ?? base);
        if (patch === null) {
          setSaveState("pending");
          return;
        }
        setSaveState("saving");
        const stored = await savePort(id, patch);
        savedRef.current = stored;
        setDocument(stored);
        setError(null);
        if (editSeq.current === seq) {
          setSaveState("saved");
          return;
        }
      }
    } catch (err) {
      setError(billingMessage(err, strings.billingSaveFailed));
      setSaveState("failed");
    } finally {
      savingRef.current = false;
    }
  }, [id, patchOf, savePort]);

  // The debounce. A form that cannot be sent yet stays "pending" and simply
  // does not schedule anything — the reason is already on the offending row.
  useEffect(() => {
    if (saveState !== "pending" || readOnly || id === undefined || document === null) return;
    if (patchOf({ header, rows }, document) === null) return;
    const timer = setTimeout(() => void save(), AUTOSAVE_MS);
    return () => clearTimeout(timer);
  }, [saveState, header, rows, readOnly, id, document, patchOf, save]);

  const create = useCallback(async (): Promise<T | null> => {
    setCreating(true);
    setError(null);
    try {
      // Blanks stay absent, as everywhere in this module: an unstated field
      // takes the server's own default rather than being written as "".
      const draft: Partial<DocumentHeader> = { customerId: editRef.current.header.customerId };
      if (editRef.current.header.reference !== "") draft.reference = editRef.current.header.reference;
      if (editRef.current.header.note !== "") draft.note = editRef.current.header.note;
      const lines = rowsDraft(editRef.current.rows);
      const createDraft: Partial<DocumentHeader> & { lines?: LineDraft[] } = { ...draft };
      if (lines !== null && lines.length > 0) createDraft.lines = lines;
      return await createPort(createDraft);
    } catch (err) {
      setError(billingMessage(err, strings.billingSaveFailed));
      return null;
    } finally {
      setCreating(false);
    }
  }, [createPort]);

  return {
    document,
    aside,
    header,
    rows,
    loading,
    missing,
    error,
    saveState,
    readOnly,
    creating,
    edit,
    saveNow: () => void save(),
    create,
    adopt,
    fail: setError,
    nextKey,
  };
}
