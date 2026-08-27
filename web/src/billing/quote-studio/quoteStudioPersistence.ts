import { strings } from "../../i18n";
import type { QuoteStudioDesign } from "./QuoteStudioDesign";
import {
  EMPTY_QUOTE_STUDIO_DESIGN,
  normalizeSavedQuoteDesign,
} from "./quoteStudioNormalization";
import {
  createQuoteTemplateDesign,
  type QuoteTemplatePreset,
} from "./quoteStudioTemplates";

const DESIGN_STORE = "quote-designs";
const DESIGN_DATABASE = "alo-quote-assets";

function legacyQuoteDesign(key: string): QuoteStudioDesign | null {
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

function openQuoteDesignDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DESIGN_DATABASE, 1);
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(DESIGN_STORE))
        request.result.createObjectStore(DESIGN_STORE);
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () =>
      reject(
        request.error ?? new Error(strings.quoteStudioDesignDatabaseError),
      );
  });
}

export async function loadQuoteStudioDesign(
  key: string,
): Promise<QuoteStudioDesign> {
  try {
    const database = await openQuoteDesignDatabase();
    const saved = await new Promise<Partial<QuoteStudioDesign> | undefined>(
      (resolve, reject) => {
        const request = database
          .transaction(DESIGN_STORE, "readonly")
          .objectStore(DESIGN_STORE)
          .get(key);
        request.onsuccess = () =>
          resolve(request.result as Partial<QuoteStudioDesign> | undefined);
        request.onerror = () => reject(request.error);
      },
    );
    database.close();
    if (saved !== undefined) return normalizeSavedQuoteDesign(saved);
  } catch {
    // Fall through to the small legacy record when IndexedDB is unavailable.
  }
  return legacyQuoteDesign(key) ?? EMPTY_QUOTE_STUDIO_DESIGN;
}

export async function saveQuoteStudioDesign(
  key: string,
  design: QuoteStudioDesign,
): Promise<void> {
  const database = await openQuoteDesignDatabase();
  await new Promise<void>((resolve, reject) => {
    const transaction = database.transaction(DESIGN_STORE, "readwrite");
    transaction.objectStore(DESIGN_STORE).put(design, key);
    transaction.oncomplete = () => resolve();
    transaction.onerror = () =>
      reject(
        transaction.error ?? new Error(strings.quoteStudioDesignSaveError),
      );
    transaction.onabort = () =>
      reject(
        transaction.error ?? new Error(strings.quoteStudioDesignSaveCancelled),
      );
  });
  database.close();
  localStorage.removeItem(key);
}

export async function saveQuoteTemplateDesign(
  quoteId: string,
  preset: QuoteTemplatePreset,
): Promise<void> {
  const key = `alo:quote-design:${quoteId}`;
  const design = createQuoteTemplateDesign(preset);
  try {
    await saveQuoteStudioDesign(key, design);
  } catch {
    localStorage.setItem(key, JSON.stringify(design));
  }
}
