import { afterEach, describe, expect, test, vi } from "vitest";

import { en } from "./en";
import { fr } from "./fr";
import { nl } from "./nl";
import { buildCatalog, getLocale, setLocale } from "./locale";
import { strings } from "./strings";

afterEach(() => {
  setLocale("en");
  vi.restoreAllMocks();
});

describe("catalog fallback", () => {
  test("a built catalog has every English key (no blanks possible)", () => {
    const catalog = buildCatalog("fr");
    for (const key of Object.keys(en)) {
      expect(catalog).toHaveProperty(key);
      expect((catalog as Record<string, unknown>)[key]).toBeDefined();
    }
  });

  test("French keys override English, untranslated keys keep English", () => {
    const catalog = buildCatalog("fr");
    // A key present in fr is translated...
    expect(catalog.moduleMail).toBe("Courrier");
    // ...and every fr key actually matches an English key (no typos /
    // stale keys that would silently never render).
    for (const key of Object.keys(fr)) {
      expect(en).toHaveProperty(key);
    }
  });

  test("Dutch keys override English, and every nl key is a real English key", () => {
    const catalog = buildCatalog("nl");
    expect(catalog.moduleMail).toBe("E-mail");
    for (const key of Object.keys(nl)) {
      expect(en).toHaveProperty(key);
    }
  });
});

describe("alo Billing is fully translated (B1.27)", () => {
  /** Every key the billing module reads. The catalogs are deliberately partial
   *  elsewhere — a half-translated Docs toolbar is a cosmetic gap — but a
   *  document a customer is asked to pay must not be half in English, so this
   *  one surface is complete or the suite is red. */
  const BILLING_AGENT_KEYS = [
    "agentActInvoiceDraft",
    "agentActQuoteToInvoice",
    "agentActPaymentReminder",
    "agentFieldCustomer",
    "agentFieldLines",
    "agentFieldQuote",
    "agentFieldInvoice",
    "agentLineCount",
    "agentInvoiceDraftNote",
    "agentQuoteToInvoiceNote",
    "agentReminderNote",
  ];
  const billingKeys = Object.keys(en).filter(
    (key) => key.startsWith("billing") || key === "moduleBilling" || BILLING_AGENT_KEYS.includes(key),
  );

  test("the key list is the real billing surface, not an empty filter", () => {
    // A typo in the filter above would make every assertion below vacuous.
    expect(billingKeys.length).toBeGreaterThan(250);
    for (const key of BILLING_AGENT_KEYS) expect(en).toHaveProperty(key);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
  ])("%s translates every billing string", (_locale, catalog) => {
    const missing = billingKeys.filter((key) => !(key in catalog));
    expect(missing).toEqual([]);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
  ])("%s keeps every interpolation a function of the same shape", (locale, catalog) => {
    for (const key of billingKeys) {
      const source = (en as Record<string, unknown>)[key];
      const translated = (catalog as Record<string, unknown>)[key];
      expect(typeof translated).toBe(typeof source);
      if (typeof source === "function" && typeof translated === "function") {
        // A translation that dropped an argument would silently print a
        // sentence with the number or the date missing.
        expect(translated.length).toBe(source.length);
      } else {
        expect(String(translated).trim()).not.toBe("");
      }
    }
    expect(locale).toMatch(/^(fr|nl)$/);
  });

  test("the translated strings really are different words", () => {
    // Guards against a "translation" pasted from English: the labels a person
    // reads first must actually change language.
    expect(buildCatalog("fr").billingInvoices).toBe("Factures");
    expect(buildCatalog("nl").billingInvoices).toBe("Facturen");
    expect(buildCatalog("fr").billingCreditNote).toBe("Avoir");
    expect(buildCatalog("nl").billingCreditNote).toBe("Creditnota");
    // …including the ones built by a function.
    expect(buildCatalog("fr").billingVatAtRate("21 %")).toBe("TVA à 21 %");
    expect(buildCatalog("nl").billingTermsDays(1)).toBe("1 dag");
    expect(buildCatalog("nl").billingTermsDays(30)).toBe("30 dagen");
  });
});

describe("alo CRM and the record history are fully translated (B2.14)", () => {
  /** The B2 surface: the sales module, the recurring-invoice arrangements it
   *  hands to billing, the agent actions that propose deals, and the history
   *  panel that says who changed a record. Same rule as billing above — a
   *  record a colleague is asked to act on must not be half in English. */
  const CRM_AGENT_KEYS = [
    "agentProposedAction",
    "agentApprove",
    "agentDiscard",
    "agentDone",
    "agentFailed",
    "agentActCreateDeal",
    "agentActMoveDeal",
    "agentActFollowup",
    "agentFieldDeal",
    "agentFieldCompany",
    "agentFieldValue",
    "agentFieldStage",
    "agentFieldLostReason",
    "agentDealFromEmailNote",
    "agentFollowupNote",
  ];
  const waveKeys = Object.keys(en).filter(
    (key) =>
      key.startsWith("crm") ||
      key.startsWith("audit") ||
      key.startsWith("billingSchedule") ||
      key.startsWith("billingCadence") ||
      key.startsWith("billingRecurring") ||
      key === "moduleCrm" ||
      CRM_AGENT_KEYS.includes(key),
  );

  test("the key list is the real B2 surface, not an empty filter", () => {
    expect(waveKeys.length).toBeGreaterThan(200);
    for (const key of CRM_AGENT_KEYS) expect(en).toHaveProperty(key);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
  ])("%s translates every B2 string", (_locale, catalog) => {
    const missing = waveKeys.filter((key) => !(key in catalog));
    expect(missing).toEqual([]);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
  ])("%s keeps every interpolation a function of the same shape", (locale, catalog) => {
    for (const key of waveKeys) {
      const source = (en as Record<string, unknown>)[key];
      const translated = (catalog as Record<string, unknown>)[key];
      expect(typeof translated).toBe(typeof source);
      if (typeof source === "function" && typeof translated === "function") {
        expect(translated.length).toBe(source.length);
      } else {
        expect(String(translated).trim()).not.toBe("");
      }
    }
    expect(locale).toMatch(/^(fr|nl)$/);
  });

  test("the translated strings really are different words", () => {
    expect(buildCatalog("fr").moduleCrm).toBe("Ventes");
    expect(buildCatalog("nl").moduleCrm).toBe("Verkoop");
    expect(buildCatalog("fr").crmStateWon).toBe("Gagnée");
    expect(buildCatalog("nl").crmStateWon).toBe("Gewonnen");
    expect(buildCatalog("fr").auditHistoryTitle).toBe("Historique");
    expect(buildCatalog("nl").auditHistoryTitle).toBe("Geschiedenis");
    // …including the ones built by a function. The French draft-document
    // noun is masculine in both branches on purpose: the sentences that
    // interpolate it ("Votre … est prêt") stay grammatical either way.
    expect(buildCatalog("fr").crmRaisedTitle(buildCatalog("fr").crmDocumentDraft("invoice"))).toBe(
      "Votre brouillon de facture est prêt",
    );
    expect(buildCatalog("fr").crmRaisedTitle(buildCatalog("fr").crmDocumentDraft("quote"))).toBe(
      "Votre brouillon de devis est prêt",
    );
    expect(buildCatalog("nl").crmDocumentDraft("invoice")).toBe("conceptfactuur");
    expect(buildCatalog("nl").billingScheduleRunDrafted(2)).toContain("2 concepten");
  });
});

describe("alo Insights is fully translated (BI1.08)", () => {
  /** The BI-1 surface: the Insights tab, its boards and tiles, the gallery of
   *  ready-made charts, the ask-to-chart dialog, and every label a chart draws
   *  with — axis buckets, table headers, statuses and age brackets. Same rule
   *  as billing and CRM above: a figure a business acts on must not come with
   *  half its words in English. */
  const insightsKeys = Object.keys(en).filter(
    (key) => key.startsWith("insights") || key === "moduleInsights",
  );

  test("the key list is the real Insights surface, not an empty filter", () => {
    expect(insightsKeys.length).toBeGreaterThan(80);
    expect(en).toHaveProperty("moduleInsights");
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
  ])("%s translates every Insights string", (_locale, catalog) => {
    const missing = insightsKeys.filter((key) => !(key in catalog));
    expect(missing).toEqual([]);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
  ])("%s keeps every interpolation a function of the same shape", (locale, catalog) => {
    for (const key of insightsKeys) {
      const source = (en as Record<string, unknown>)[key];
      const translated = (catalog as Record<string, unknown>)[key];
      expect(typeof translated).toBe(typeof source);
      if (typeof source === "function" && typeof translated === "function") {
        expect(translated.length).toBe(source.length);
      } else {
        expect(String(translated).trim()).not.toBe("");
      }
    }
    expect(locale).toMatch(/^(fr|nl)$/);
  });

  test("the translated strings really are different words", () => {
    expect(buildCatalog("fr").moduleInsights).toBe("Analyses");
    expect(buildCatalog("nl").moduleInsights).toBe("Inzichten");
    expect(buildCatalog("fr").insightsAddChart).toBe("Ajouter un graphique");
    expect(buildCatalog("nl").insightsAddChart).toBe("Grafiek toevoegen");
    // A period abbreviation is a translation too: an axis reading "Q1" in
    // French, or "W03" in Dutch, is English leaking onto a chart.
    expect(buildCatalog("fr").insightsQuarter(1, 2026)).toBe("T1 2026");
    expect(buildCatalog("nl").insightsQuarter(1, 2026)).toBe("K1 2026");
    expect(buildCatalog("fr").insightsWeek(3, 2026)).toBe("S3 2026");
    // …and the plural branch of the unconverted-documents note, which the
    // English catalog builds from two separate sentences.
    expect(buildCatalog("fr").insightsNoteUnconverted(1)).toContain("1 document n’a pas pu");
    expect(buildCatalog("fr").insightsNoteUnconverted(3)).toContain("3 documents n’ont pas pu");
    expect(buildCatalog("nl").insightsNoteUnconverted(1)).toContain("1 document kon niet");
    expect(buildCatalog("nl").insightsNoteUnconverted(3)).toContain("3 documenten konden niet");
  });

  test("the overview's chart titles match the words the server seeds", () => {
    // The Business overview is written by `insights_gallery.rs` in the
    // reader's language, and the gallery offers the same seven charts from
    // this catalog. If the two disagree, pinning a chart a tenant already has
    // looks like a different chart. Keep this list in step with SeedWords.
    expect(buildCatalog("fr").insightsGalleryOutstanding).toBe("Créances en cours");
    expect(buildCatalog("fr").insightsGalleryWonThisMonth).toBe("Gagné ce mois-ci");
    expect(buildCatalog("fr").insightsGalleryRevenueByMonth).toBe("Chiffre d’affaires par mois");
    expect(buildCatalog("fr").insightsGalleryOverdueAging).toBe("Retards par ancienneté");
    expect(buildCatalog("fr").insightsGalleryPipelineByStage).toBe("Pipeline par étape");
    expect(buildCatalog("fr").insightsGalleryVatByQuarter).toBe("TVA par trimestre");
    expect(buildCatalog("fr").insightsGalleryWinRateByQuarter).toBe(
      "Taux de réussite par trimestre",
    );
    expect(buildCatalog("nl").insightsGalleryOutstanding).toBe("Openstaand");
    expect(buildCatalog("nl").insightsGalleryWonThisMonth).toBe("Gewonnen deze maand");
    expect(buildCatalog("nl").insightsGalleryRevenueByMonth).toBe("Omzet per maand");
    expect(buildCatalog("nl").insightsGalleryOverdueAging).toBe("Achterstand per ouderdom");
    expect(buildCatalog("nl").insightsGalleryPipelineByStage).toBe("Pipeline per fase");
    expect(buildCatalog("nl").insightsGalleryVatByQuarter).toBe("Btw per kwartaal");
    expect(buildCatalog("nl").insightsGalleryWinRateByQuarter).toBe("Winstpercentage per kwartaal");
  });
});

describe("runtime switching", () => {
  test("strings proxy reflects the active locale live", () => {
    expect(getLocale()).toBe("en");
    expect(strings.compose).toBe("Compose");
    setLocale("fr");
    expect(getLocale()).toBe("fr");
    expect(strings.compose).toBe("Écrire");
    // Interpolation functions switch too.
    expect(strings.selectedCount(2)).toBe("2 sélectionnés");
    setLocale("en");
    expect(strings.compose).toBe("Compose");
  });

  test("setLocale persists the choice to localStorage", () => {
    // jsdom's localStorage is a non-functional stub here; install a
    // real in-memory one to observe the persistence contract.
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
      removeItem: (k: string) => void store.delete(k),
      clear: () => store.clear(),
    });
    setLocale("fr");
    expect(store.get("alo.locale")).toBe("fr");
    setLocale("en");
    expect(store.get("alo.locale")).toBe("en");
    vi.unstubAllGlobals();
  });
});
