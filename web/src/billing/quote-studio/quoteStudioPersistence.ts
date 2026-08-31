// Where a quotation's design lives: on the server, with the quote.
//
// The studio used to keep its layout in the browser that composed it
// (IndexedDB, and before that localStorage), which meant the printed document
// carried none of it. The design is now a record of the quote
// (`PUT /billing/quotes/{id}/design`), read by the print and the PDF. This
// module is the one place the studio loads and saves it, and the one place the
// old browser copies are still read — once, to move them to the server — so
// nobody who designed a quotation last month loses that work.
import type { BillingApi } from "../api";
import type { QuoteStudioDesign } from "./QuoteStudioDesign";
import {
  EMPTY_QUOTE_STUDIO_DESIGN,
  normalizeSavedQuoteDesign,
} from "./quoteStudioNormalization";
import {
  createQuoteTemplateDesign,
  type QuoteTemplatePreset,
} from "./quoteStudioTemplates";

export type BillingDocumentDesignKind = "quote" | "invoice";

const LEGACY_STORE = "quote-designs";
const LEGACY_DATABASE = "alo-quote-assets";

/** The key both browser stores used for a quote. */
function legacyKey(quoteId: string): string {
  return `alo:quote-design:${quoteId}`;
}

function legacyLocalDesign(key: string): QuoteStudioDesign | null {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return null;
    return normalizeSavedQuoteDesign(
      JSON.parse(raw) as Partial<QuoteStudioDesign>,
    );
  } catch {
    return null;
  }
}

function openLegacyDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(LEGACY_DATABASE, 1);
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(LEGACY_STORE))
        request.result.createObjectStore(LEGACY_STORE);
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("indexeddb"));
  });
}

async function legacyIndexedDesign(
  key: string,
): Promise<QuoteStudioDesign | null> {
  try {
    const database = await openLegacyDatabase();
    const saved = await new Promise<Partial<QuoteStudioDesign> | undefined>(
      (resolve, reject) => {
        const request = database
          .transaction(LEGACY_STORE, "readonly")
          .objectStore(LEGACY_STORE)
          .get(key);
        request.onsuccess = () =>
          resolve(request.result as Partial<QuoteStudioDesign> | undefined);
        request.onerror = () => reject(request.error);
      },
    );
    database.close();
    return saved === undefined ? null : normalizeSavedQuoteDesign(saved);
  } catch {
    return null;
  }
}

/** Forgets the browser copies once the server holds the design. */
async function forgetLegacy(key: string): Promise<void> {
  try {
    localStorage.removeItem(key);
  } catch {
    // Nothing to forget, or no storage at all.
  }
  try {
    const database = await openLegacyDatabase();
    await new Promise<void>((resolve) => {
      const transaction = database.transaction(LEGACY_STORE, "readwrite");
      transaction.objectStore(LEGACY_STORE).delete(key);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => resolve();
      transaction.onabort = () => resolve();
    });
    database.close();
  } catch {
    // The database is gone or unavailable; there is nothing left to forget.
  }
}

/** A design saved by this browser before designs lived on the server. */
async function legacyDesign(quoteId: string): Promise<QuoteStudioDesign | null> {
  const key = legacyKey(quoteId);
  return (await legacyIndexedDesign(key)) ?? legacyLocalDesign(key);
}

/**
 * The quote's design from the server. A quote never designed there starts
 * from what this browser saved for it, if anything — moved to the server on
 * the spot, so the printed document carries it from now on — and otherwise
 * from the blank design.
 *
 * Never rejects: a design must always load, or the quotation cannot be
 * edited. If the server cannot be reached, the browser copy or the blank
 * design is what the studio gets, and the next save reports the failure.
 */
export async function loadQuoteStudioDesign(
  api: BillingApi,
  quoteId: string,
  kind: BillingDocumentDesignKind = "quote",
): Promise<QuoteStudioDesign> {
  let stored: unknown = null;
  let reachable = true;
  try {
    stored = (
      await (kind === "invoice"
        ? api.invoiceDesign(quoteId)
        : api.quoteDesign(quoteId))
    ).design;
  } catch {
    reachable = false;
  }
  if (stored !== null && typeof stored === "object") {
    return normalizeSavedQuoteDesign(stored as Partial<QuoteStudioDesign>);
  }
  const legacy = kind === "quote" ? await legacyDesign(quoteId) : null;
  if (legacy === null) return EMPTY_QUOTE_STUDIO_DESIGN;
  if (reachable) {
    try {
      await api.saveQuoteDesign(quoteId, legacy);
      await forgetLegacy(legacyKey(quoteId));
    } catch {
      // A frozen (sent) offer refuses the write; the browser copy stays as
      // the only record, and the studio shows it read-only anyway.
    }
  }
  return legacy;
}

/** Replaces the quote's design on the server. Rejects with the server's
 *  answer — a sent offer refuses (`409`), an oversized design refuses. */
export function saveQuoteStudioDesign(
  api: BillingApi,
  quoteId: string,
  design: QuoteStudioDesign,
  kind: BillingDocumentDesignKind = "quote",
): Promise<void> {
  return kind === "invoice"
    ? api.saveInvoiceDesign(quoteId, design)
    : api.saveQuoteDesign(quoteId, design);
}

/** Starts a fresh quote from one of the studio's templates. */
export function saveQuoteTemplateDesign(
  api: BillingApi,
  quoteId: string,
  preset: QuoteTemplatePreset,
): Promise<void> {
  return saveQuoteStudioDesign(api, quoteId, createQuoteTemplateDesign(preset));
}
