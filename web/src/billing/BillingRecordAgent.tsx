// The record's agent on a billing record (AW.7).
//
// A thin adapter, not a second panel: `RecordAgentPanel` renders every module's
// record in this workspace, and Billing mounts the same component. What lives
// here is the one thing the shared panel cannot know — where a billing record
// came from — and Billing answers it from the record itself.
//
// **An invoice keeps its provenance.** It says whether it was raised from an
// accepted offer (`quoteId`), by a standing arrangement on its due date
// (`scheduleId`), or to correct a document the customer already holds
// (`creditsInvoiceId`). Those are read straight off the document.
//
// **A quote and a customer keep none, and say so.** Both carry `createdBy`,
// but it holds an opaque subject id, and an origin is cited in words or not at
// all — the same refusal the CRM drawer makes for the same field. The audit
// trail would name the person, and this file did read it until the walk showed
// the trail is empty for anything the web app does: the API is mounted twice
// and the browser calls the `/api` copy, where the audit layer sees a matched
// template beginning `/api` and files nothing. That is a backend fault, and
// this track adds no backend — it is reported in the journal, not patched
// here. When it is fixed, a creator origin belongs in this function.
//
// No money is computed here and none is read: the panel shows provenance, the
// verbs the registry already offers, and an ask. Every total, VAT figure and
// due date on these screens stays the ledger's (ADR 0011).
import { RecordAgentPanel, type RecordOrigin } from "../agents";

/** What any billing document has that could say where it came from. Written
 *  as an open shape so one function serves the invoice editor and the quote
 *  editor: a quote simply carries none of the three. */
export interface DocumentProvenance {
  /** The offer this invoice was raised from, when it was accepted. */
  quoteId?: string | null;
  /** The recurring arrangement whose due run raised this draft (B2.11). */
  scheduleId?: string | null;
  /** The invoice this credit note corrects. */
  creditsInvoiceId?: string | null;
}

/**
 * Where a billing document came from, as the record itself carries it, or
 * `null` when it does not say.
 *
 * The three are mutually exclusive in the store — a credit note mirrors an
 * invoice, a converted offer raises a fresh draft, a schedule raises its own —
 * and they are checked most-specific first so a document that somehow carried
 * two would still be described by the nearer of them.
 */
export function documentOrigin(
  document: DocumentProvenance,
): RecordOrigin | null {
  if (document.creditsInvoiceId != null && document.creditsInvoiceId !== "") {
    return { kind: "correction", id: document.creditsInvoiceId, label: null };
  }
  if (document.quoteId != null && document.quoteId !== "") {
    // The invoice knows the offer's id, never its number, so the sentence is
    // the unnamed one and the citation is the link back to the offer.
    return { kind: "quote", id: document.quoteId, label: null };
  }
  if (document.scheduleId != null && document.scheduleId !== "") {
    return { kind: "schedule", id: document.scheduleId, label: null };
  }
  return null;
}

interface Props {
  /** The record's kind as the verb catalogue spells it: `invoiceOwed`,
   *  `invoice`, `quoteOpen`, `quote`, `customer`. */
  recordKind: string;
  recordId: string;
  /** What to call the record when asking about it — the number on screen, or
   *  the customer's name. */
  recordLabel: string;
  /** Provenance the record itself carries; `null` when it says nothing. */
  origin?: RecordOrigin | null;
  /** The detail view's chance to close itself before the panel navigates. */
  onBeforeNavigate?: () => void;
}

/** The shared panel, addressed the way Billing's records are addressed. One
 *  mount point rather than three copies of `product="billing"`. */
export function BillingRecordAgent({
  recordKind,
  recordId,
  recordLabel,
  origin = null,
  onBeforeNavigate,
}: Props) {
  return (
    <RecordAgentPanel
      product="billing"
      recordKind={recordKind}
      recordId={recordId}
      recordLabel={recordLabel}
      origin={origin}
      {...(onBeforeNavigate === undefined ? {} : { onBeforeNavigate })}
    />
  );
}
