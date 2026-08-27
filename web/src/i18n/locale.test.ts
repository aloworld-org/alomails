import { afterEach, describe, expect, test, vi } from "vitest";

import { de } from "./de";
import { en } from "./en";
import { fr } from "./fr";
import { nl } from "./nl";
import { buildCatalog, getLocale, setLocale } from "./locale";
import { strings } from "./strings";
import { UNTRANSLATED } from "./untranslated";

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

  test("German keys override English, and every de key is a real English key", () => {
    const catalog = buildCatalog("de");
    expect(catalog.moduleMail).toBe("E-Mail");
    for (const key of Object.keys(de)) {
      expect(en).toHaveProperty(key);
    }
    // German shipped module by module (M4.1) and is complete since
    // tranche 9; the drift ratchet at the bottom of this file now holds
    // it to parity alongside Dutch and French.
    expect(Object.keys(de).length).toBe(Object.keys(en).length);
  });
});

describe("German ships complete modules (M4.1, tranches 1–9: the whole catalog, Sites commerce half last)", () => {
  /** The sections `de.ts` claims to cover, by key prefix. The catalog is
   *  allowed to be partial across *modules* — the fallback shows English —
   *  but never inside one: a reading pane that mixes German buttons with
   *  English menus reads as broken, not as untranslated. A new English key
   *  in any of these families must land with German in the same change.
   *  Tranche 1 is the mail daily-driver surface; tranche 2 adds Docs (block
   *  editor, technical authoring, formatting toolbar), Drive + Spaces,
   *  Sheets, the Office embed, the search overlay and the Drive picker;
   *  tranche 3 adds Chat and Meet plus the exact-match generics those two
   *  surfaces are first to use (anchored with `$` so they claim one key,
   *  not a family); tranche 4 adds the admin console, the control plane,
   *  the invitation page, AI providers (whose kind/desc singletons are
   *  anchored), the record-history panel, and the compose recipient
   *  strays; tranche 5 adds Billing, CRM and Insights entire (their agent
   *  cards are enforced by those modules' own fully-translated describes
   *  below, which German joined in the same tranche); tranche 6 adds
   *  Projects and Finance entire, their agent cards enforced the same way
   *  (B3.11 and B4.15 below); tranche 7 adds Inventory, HR and Campaigns
   *  entire (incl. the stranger-facing unsubscribe page under the
   *  `campaign` singular prefix), the Drive Base family, and the whole
   *  `agent` family — the assistant's mail/calendar/chat/Drive cards were
   *  the last agent keys out, so the anchored agentApprove/agentDiscard
   *  singletons widened to the plain prefix. Tranche 8 adds the Sites
   *  builder half; tranche 9 finishes the Sites commerce half (catalog,
   *  shop, booking, tickets, collections, custom code, orders, domains),
   *  so `sites` is claimed whole and the catalog is complete — the drift
   *  ratchet at the bottom of this file now holds German to full parity,
   *  and this list remains as the per-module map of how it got there. */
  const SHIPPED_PREFIXES =
    /^(agenda|task|mail|compose|flag|folder|filter|spam|snooze|unsubscribe|appPassword|delegate|sharing|shared|categor|transfer|contact|import|signup|reset|settings|brand|home|module|rsvp|error|twoFactor|recovery|doc|drive|sheet|office|picker|search|ai|eq|spec|tb|ref|block|heading|para|table|chart|code|insert|font|size|align|text|style|highlight|strikethrough|bullet|numbered|horizontal|clear|close|cancel|chat|meet|admin|audit|control|domain|group|invite|overview|provider|security|tenant|user|dkim|kind|access|billing|crm|insights|projects|finance|inventory|hr|campaign|base|agent|sites|add$|save$|deleteLabel$|aloDesc$|anthropicDesc$|openaiDesc$|mistralDesc$|ollamaDesc$|customDesc$|builtInTag$|connectTitle$|configureTitle$|removeRecipient$|recipientCount$|archiveUnavailable$)/;
  const shippedKeys = Object.keys(en).filter((key) =>
    SHIPPED_PREFIXES.test(key),
  );

  test("the key list is the real shipped surface, not an empty filter", () => {
    expect(shippedKeys.length).toBeGreaterThan(5000);
    expect(Object.keys(de).length).toBeGreaterThan(5100);
  });

  test("every shipped-module string exists in German", () => {
    const missing = shippedKeys.filter((key) => !(key in de));
    expect(
      missing,
      `These keys belong to a module de.ts ships and need German:\n  ${missing.join("\n  ")}`,
    ).toEqual([]);
  });

  test("German keeps every interpolation a function of the same shape", () => {
    for (const key of Object.keys(de)) {
      const source = (en as Record<string, unknown>)[key];
      const translated = (de as Record<string, unknown>)[key];
      expect(typeof translated).toBe(typeof source);
      if (typeof source === "function" && typeof translated === "function") {
        // A translation that dropped an argument would silently print a
        // sentence with the number or the date missing.
        expect(translated.length).toBe(source.length);
      } else {
        expect(String(translated).trim()).not.toBe("");
      }
    }
  });

  test("the translated strings really are different words", () => {
    const catalog = buildCatalog("de");
    expect(catalog.compose).toBe("Schreiben");
    expect(catalog.reply).toBe("Antworten");
    expect(catalog.replyAll).toBe("Allen antworten");
    expect(catalog.forward).toBe("Weiterleiten");
    expect(catalog.moduleAgenda).toBe("Kalender");
    expect(catalog.settingsAppPasswords).toBe("App-Passwörter");
    // …including the ones built by a function, in both plural branches.
    expect(catalog.selectedCount(1)).toBe("1 ausgewählt");
    expect(catalog.selectedCount(3)).toBe("3 ausgewählt");
    expect(catalog.agendaEventCount(1)).toBe("1 Termin");
    expect(catalog.agendaEventCount(4)).toBe("4 Termine");
    expect(catalog.contactsImported(1, 0)).toBe("1 Kontakt importiert.");
    expect(catalog.contactsImported(2, 1)).toBe(
      "2 Kontakte importiert (1 übersprungen).",
    );
    // Tranche 2: Docs/Drive/Sheets, in both plural branches where there are
    // branches, and the searchKind mapping in German words.
    expect(catalog.driveTrash).toBe("Papierkorb");
    expect(catalog.docsUntitled).toBe("Unbenanntes Dokument");
    expect(catalog.sheetBold).toBe("Fett");
    expect(catalog.driveSelected(1)).toBe("1 Element ausgewählt");
    expect(catalog.driveSelected(5)).toBe("5 Elemente ausgewählt");
    expect(catalog.searchKind("task")).toBe("Aufgabe");
    expect(catalog.searchKind("doc")).toBe("Doc");
    // Tranche 3: Chat/Meet, in both plural branches, incl. the two-argument
    // agent record and the consent count whose verb changes number.
    expect(catalog.chatSend).toBe("Senden");
    expect(catalog.meetLeave).toBe("Verlassen");
    expect(catalog.chatReplies(1)).toBe("1 Antwort");
    expect(catalog.chatReplies(3)).toBe("3 Antworten");
    expect(catalog.chatMentionsYou(1)).toBe("1 Nachricht erwähnt Sie");
    expect(catalog.chatMentionsYou(2)).toBe("2 Nachrichten erwähnen Sie");
    expect(catalog.meetLiveCount(1)).toBe("1 Meeting");
    expect(catalog.meetLiveCount(2)).toBe("2 Meetings");
    expect(catalog.meetConsentCount(1)).toBe("1 hat zugestimmt");
    expect(catalog.meetConsentCount(3)).toBe("3 haben zugestimmt");
    expect(catalog.chatAgentRecord(1, 0)).toBe("1 Antwort");
    expect(catalog.chatAgentRecord(2, 1)).toBe(
      "2 Antworten · 1 Aktion genehmigt",
    );
    // The button that opens the calendar module says what the module is
    // called in German — Kalender, matching moduleAgenda — not "Agenda".
    expect(catalog.meetOpenAgenda).toBe("Kalender öffnen");
    // Tranche 4: admin console + control plane, in both plural branches,
    // incl. the two-argument usage lines.
    expect(catalog.adminOverview).toBe("Übersicht");
    expect(catalog.controlTitle).toBe("Plattformverwaltung");
    expect(catalog.tenantSuspend).toBe("Sperren");
    expect(catalog.groupMemberCount(1)).toBe("1 Mitglied");
    expect(catalog.groupMemberCount(5)).toBe("5 Mitglieder");
    expect(catalog.userUsage(1, "2 GB")).toBe("1 Nachricht · 2 GB");
    expect(catalog.userUsage(3, "2 GB")).toBe("3 Nachrichten · 2 GB");
    expect(catalog.providerTestOk(1)).toBe(
      "Verbindung geprüft — 1 Modell erreichbar",
    );
    expect(catalog.providerTestOk(4)).toBe(
      "Verbindung geprüft — 4 Modelle erreichbar",
    );
    expect(catalog.auditHistoryTitle).toBe("Verlauf");
    // Reject (a timesheet sent back) and Decline (a quote the customer
    // said no to) are two different acts and must stay two words.
    expect(catalog.auditActionReject).not.toBe(catalog.auditActionDecline);
    // The confirm that deletes an organization must really warn in German.
    expect(catalog.tenantDeleteConfirm("Acme")).toContain(
      "lässt sich nicht rückgängig machen",
    );
    // Tranche 5: Billing/CRM/Insights. The one-word rules tranche 4 set up:
    // a document's status chip and the history's verb are the same word, and
    // a declined quote takes the decline word, never the reject word.
    expect(catalog.billingStatusIssued).toBe(catalog.auditActionIssue);
    expect(catalog.billingStatusIssued).toBe("Ausgestellt");
    expect(catalog.billingQuoteStatusDeclined).toBe(catalog.auditActionDecline);
    expect(catalog.billingQuoteStatusAccepted).toBe(catalog.auditActionAccept);
    expect(catalog.billingCreditNote).toBe("Gutschrift");
    expect(catalog.billingTermsDays(1)).toBe("1 Tag");
    expect(catalog.billingTermsDays(30)).toBe("30 Tage");
    expect(catalog.billingVatAtRate("21 %")).toBe("MwSt. 21 %");
    // The CRM handoff noun is capitalized, so both sentences that
    // interpolate it stay orthographically correct German.
    expect(catalog.crmDocumentDraft("invoice")).toBe("Rechnungsentwurf");
    expect(catalog.crmRaiseTitle(catalog.crmDocumentDraft("quote"))).toBe(
      "Angebotsentwurf erstellen",
    );
    expect(catalog.crmRaisedTitle(catalog.crmDocumentDraft("invoice"))).toBe(
      "Ihr Rechnungsentwurf ist bereit",
    );
    expect(catalog.crmStateWon).toBe("Gewonnen");
    // Insights: the German calendar week is KW, quarters keep Q, and the
    // truncation note names the same bucket the table shows.
    expect(catalog.insightsQuarter(1, 2026)).toBe("Q1 2026");
    expect(catalog.insightsWeek(3, 2026)).toBe("KW 3 2026");
    expect(catalog.insightsTruncated).toContain(
      `„${catalog.insightsBucketOther}“`,
    );
    expect(catalog.insightsNoteUnconverted(1)).toContain(
      "1 Dokument konnte nicht",
    );
    expect(catalog.insightsNoteUnconverted(3)).toContain(
      "3 Dokumente konnten nicht",
    );
    // Tranche 6: Projects/Finance. A week's status chip carries the history
    // verb (sent back = zurückgewiesen, approved = genehmigt), and German
    // durations are written the way a German timesheet writes them.
    expect(catalog.projectsWeekRejected).toBe(catalog.auditActionReject);
    expect(catalog.projectsWeekApproved).toBe(catalog.auditActionApprove);
    expect(catalog.projectsHoursShort(7)).toBe("7 Std.");
    expect(catalog.projectsMinutesShort(30)).toBe("30 Min.");
    expect(catalog.projectsPercent(60)).toBe("60 %");
    expect(catalog.projectsSuggestionsWaiting(1)).toContain(
      "1 Vorschlag wartet",
    );
    expect(catalog.projectsSuggestionsWaiting(3)).toContain(
      "3 Vorschläge warten",
    );
    // Finance: the four reports carry the names a German accountant looks
    // for, and no sentence makes a verb agree with an interpolated amount.
    expect(catalog.financeReportPl).toBe("Gewinn- und Verlustrechnung");
    expect(catalog.financeReportBalance).toBe("Bilanz");
    expect(catalog.financeChartLoadFailed).toContain("Kontenplan");
    expect(catalog.financeBankStillOwedIs("1,00 €")).toBe("1,00 € noch offen");
    expect(catalog.financeBankPickSubtitle("1,00 €")).toContain(
      "Eingegangen: 1,00 €",
    );
    expect(catalog.financeReportUnbalanced("1,00 €")).toContain(
      "ein Betrag von 1,00 € ist unerklärt",
    );
    expect(catalog.financeMarkPaidBackSubtitle("Marie", "1,00 €")).toBe(
      "1,00 € zurück an Marie.",
    );
    expect(catalog.financeReportOpenDocuments(1)).toBe("1 offenes Dokument");
    expect(catalog.financeReportOpenDocuments(3)).toBe("3 offene Dokumente");
    // Tranche 7: Inventory/HR/Campaigns/Base + the agent tail. German trade
    // language splits what English shares — a purchase order is a Bestellung,
    // a sales order an Auftrag — and the movement reasons carry the
    // warehouse's own nouns, not translations of our English.
    expect(catalog.inventoryNewPurchaseOrder).toBe("Neue Bestellung");
    expect(catalog.inventoryNewSalesOrder).toBe("Neuer Auftrag");
    expect(catalog.inventoryReasonReceipt).toBe("Wareneingang");
    expect(catalog.inventoryReasonCount).toBe("Inventur");
    expect(catalog.inventoryReasonShrinkage).toBe("Schwund");
    // The draft-invoice word is Billing's own, in both modules that raise one.
    expect(catalog.inventoryDraftInvoice).toBe(catalog.billingDraftInvoice);
    expect(catalog.inventoryInvoiceDrafted).toContain(catalog.moduleBilling);
    // HR: sending a week back is the same act (and word) as in Projects, a
    // candidate is told "nicht berücksichtigt" — the rejection letter's own
    // phrase, never the form's reject word — and Ausgeschieden is said
    // plainly.
    expect(catalog.hrSendBack).toBe(catalog.projectsReject);
    expect(catalog.hrStageRejected).toBe("Nicht berücksichtigt");
    expect(catalog.hrStageRejected).not.toBe(catalog.auditActionReject);
    expect(catalog.hrLeft).toBe("Ausgeschieden");
    expect(catalog.hrKindPermanent).toBe("Unbefristet");
    // Plural branches across the three new families.
    expect(catalog.agentReorderDrafted(1)).toBe("1 Bestellentwurf");
    expect(catalog.agentReorderDrafted(3)).toBe("3 Bestellentwürfe");
    expect(catalog.hrPeopleCount(1)).toBe("1 Person");
    expect(catalog.hrPeopleCount(9)).toBe("9 Personen");
    expect(catalog.hrWaitingCount(1)).toBe("1 wartet");
    expect(catalog.hrWaitingCount(4)).toBe("4 warten");
    expect(catalog.campaignsTallyMailable(3, 5)).toBe(
      "3 von 5 Personen werden angeschrieben",
    );
    // The unsubscribe page says stop, not "manage preferences".
    expect(catalog.campaignUnsubscribeLinkText).toBe("Abmelden");
    expect(catalog.campaignUnsubscribeStopAll).toBe(
      "Senden Sie mir gar nichts mehr",
    );
    // Tranche 8: the Sites builder half. Live is Online (the web word a
    // visitor uses), unpublishing says what it does, and the section the
    // funnel shares with CRM keeps CRM's own word for a deal.
    expect(catalog.sitesStatusLive).toBe("Online");
    expect(catalog.sitesUnpublish).toBe("Vom Netz nehmen");
    expect(catalog.sitesSectionHero).toBe("Aufmacher");
    expect(catalog.sitesSectionTestimonials).toBe("Kundenstimmen");
    expect(catalog.sitesFunnelColDeals).toBe(catalog.crmDealsTable);
    // The palette's position words keep their German capitalization: the
    // English template lowercases its position, which would turn „Ans Ende“
    // into „ans ende“ — the German template must not.
    expect(
      catalog.sitesPaletteAdd(catalog.sitesSectionHero, catalog.sitesPaletteAtEnd),
    ).toBe("Aufmacher hinzufügen — Ans Ende");
    // A published version is a Fassung named by its date, in both branches.
    expect(catalog.sitesHistoryVersionOf("12. Mai 2026")).toBe(
      "Fassung vom 12. Mai 2026",
    );
    expect(catalog.sitesHistoryUnchangedPages(1)).toBe("1 Seite bleibt gleich");
    expect(catalog.sitesHistoryUnchangedPages(3)).toBe(
      "3 Seiten bleiben gleich",
    );
    // Percentages take the DIN space, as everywhere else in German alo.
    expect(catalog.sitesImageFocalAt(30, 40)).toBe(
      "Fokuspunkt 30 % von links, 40 % von oben",
    );
    // Plural branches across the builder surface.
    expect(catalog.sitesCountEntries(1)).toBe("1 Eintrag");
    expect(catalog.sitesCountEntries(4)).toBe("4 Einträge");
    expect(catalog.sitesTranslationPublishHint(1)).toContain("1 Übersetzung zeigt");
    expect(catalog.sitesTranslationPublishHint(2)).toContain(
      "2 Übersetzungen zeigen",
    );
    // Tranche 9: the Sites commerce half. A website visitor places a
    // Bestellung — the everyday B2C word; the formal Auftrag stays the
    // order book's (inventoryNewSalesOrder above). A ticketed event is a
    // Veranstaltung, never a Termin — the calendar keeps that word.
    expect(catalog.sitesOrders).toBe("Bestellungen");
    expect(catalog.sitesOrders).not.toBe(catalog.inventoryNewSalesOrder);
    expect(catalog.sitesNewTicketEvent).toBe("Neue Veranstaltung");
    expect(catalog.sitesCatalogNoPrice).toBe("Preis auf Anfrage");
    expect(catalog.sitesCatalogSoldOut).toBe("Ausverkauft");
    // The shop-setup proposal is approved with the agent cards' own word,
    // while a domain price — a sum of money — is freigegeben.
    expect(catalog.sitesShopSetupApprove(1)).toContain(catalog.agentApprove);
    expect(catalog.sitesDomainApproveAction("12,00 €")).toBe(
      "12,00 € freigeben",
    );
    // The proposal quotes the item's name in German quotes.
    expect(catalog.sitesShopSetupInclude("Brot")).toBe("„Brot“ anlegen");
    // Plural branches across the commerce surface.
    expect(catalog.sitesTicketSectionOnSale(1)).toBe(
      "1 Veranstaltung ist im Verkauf.",
    );
    expect(catalog.sitesTicketSectionOnSale(3)).toBe(
      "3 Veranstaltungen sind im Verkauf.",
    );
    expect(catalog.sitesShopUnits(1)).toBe("1 Stück");
    expect(catalog.sitesShopUnits(5)).toBe("5 Stück");
    expect(catalog.sitesTicketSeatsCell(2, 3, 5)).toBe(
      "2 verkauft · 3 von 5 frei",
    );
    expect(catalog.sitesDomainYearsOption(1)).toBe("1 Jahr");
    expect(catalog.sitesDomainYearsOption(2)).toBe("2 Jahre");
    // The renewal price is said beside the first-year price, both branches.
    expect(catalog.sitesDomainTermPrice("12,00 €", 1)).toBe(
      "12,00 € für das erste Jahr",
    );
    expect(catalog.sitesDomainTermPrice("24,00 €", 2)).toBe(
      "24,00 € für die ersten 2 Jahre",
    );
  });

  test("the spam-banner fallback declines correctly in both sentences", () => {
    // `spamSenderFallback` is interpolated into two different sentences, so
    // its case must fit both — the reason both are phrased with dative "von".
    const catalog = buildCatalog("de");
    expect(catalog.spamReasonDmarc(catalog.spamSenderFallback)).toContain(
      "von der Absenderdomain stammt",
    );
    expect(catalog.spamReasonSpf(catalog.spamSenderFallback)).toContain(
      "von der Absenderdomain nicht zum E-Mail-Versand",
    );
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
    (key) =>
      key.startsWith("billing") ||
      key === "moduleBilling" ||
      BILLING_AGENT_KEYS.includes(key),
  );

  test("the key list is the real billing surface, not an empty filter", () => {
    // A typo in the filter above would make every assertion below vacuous.
    expect(billingKeys.length).toBeGreaterThan(250);
    for (const key of BILLING_AGENT_KEYS) expect(en).toHaveProperty(key);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])("%s translates every billing string", (_locale, catalog) => {
    const missing = billingKeys.filter((key) => !(key in catalog));
    expect(missing).toEqual([]);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])(
    "%s keeps every interpolation a function of the same shape",
    (locale, catalog) => {
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
      expect(locale).toMatch(/^(fr|nl|de)$/);
    },
  );

  test("the translated strings really are different words", () => {
    // Guards against a "translation" pasted from English: the labels a person
    // reads first must actually change language.
    expect(buildCatalog("fr").billingInvoices).toBe("Factures");
    expect(buildCatalog("nl").billingInvoices).toBe("Facturen");
    expect(buildCatalog("de").billingInvoices).toBe("Rechnungen");
    expect(buildCatalog("fr").billingCreditNote).toBe("Avoir");
    expect(buildCatalog("nl").billingCreditNote).toBe("Creditnota");
    expect(buildCatalog("de").billingCreditNote).toBe("Gutschrift");
    // …including the ones built by a function.
    expect(buildCatalog("fr").billingVatAtRate("21 %")).toBe("TVA à 21 %");
    expect(buildCatalog("nl").billingTermsDays(1)).toBe("1 dag");
    expect(buildCatalog("nl").billingTermsDays(30)).toBe("30 dagen");
    expect(buildCatalog("de").billingTermsDays(1)).toBe("1 Tag");
    expect(buildCatalog("de").billingTermsDays(30)).toBe("30 Tage");
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
    ["de", de],
  ])("%s translates every B2 string", (_locale, catalog) => {
    const missing = waveKeys.filter((key) => !(key in catalog));
    expect(missing).toEqual([]);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])(
    "%s keeps every interpolation a function of the same shape",
    (locale, catalog) => {
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
      expect(locale).toMatch(/^(fr|nl|de)$/);
    },
  );

  test("the translated strings really are different words", () => {
    expect(buildCatalog("fr").moduleCrm).toBe("Ventes");
    expect(buildCatalog("nl").moduleCrm).toBe("Verkoop");
    expect(buildCatalog("de").moduleCrm).toBe("Vertrieb");
    expect(buildCatalog("fr").crmStateWon).toBe("Gagnée");
    expect(buildCatalog("nl").crmStateWon).toBe("Gewonnen");
    expect(buildCatalog("de").crmStateWon).toBe("Gewonnen");
    expect(buildCatalog("fr").auditHistoryTitle).toBe("Historique");
    expect(buildCatalog("nl").auditHistoryTitle).toBe("Geschiedenis");
    expect(buildCatalog("de").auditHistoryTitle).toBe("Verlauf");
    // …including the ones built by a function. The French draft-document
    // noun is masculine in both branches on purpose: the sentences that
    // interpolate it ("Votre … est prêt") stay grammatical either way.
    expect(
      buildCatalog("fr").crmRaisedTitle(
        buildCatalog("fr").crmDocumentDraft("invoice"),
      ),
    ).toBe("Votre brouillon de facture est prêt");
    expect(
      buildCatalog("fr").crmRaisedTitle(
        buildCatalog("fr").crmDocumentDraft("quote"),
      ),
    ).toBe("Votre brouillon de devis est prêt");
    expect(buildCatalog("nl").crmDocumentDraft("invoice")).toBe(
      "conceptfactuur",
    );
    expect(buildCatalog("nl").billingScheduleRunDrafted(2)).toContain(
      "2 concepten",
    );
    expect(buildCatalog("de").billingScheduleRunDrafted(2)).toContain(
      "2 Entwürfe",
    );
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
    ["de", de],
  ])("%s translates every Insights string", (_locale, catalog) => {
    const missing = insightsKeys.filter((key) => !(key in catalog));
    expect(missing).toEqual([]);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])(
    "%s keeps every interpolation a function of the same shape",
    (locale, catalog) => {
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
      expect(locale).toMatch(/^(fr|nl|de)$/);
    },
  );

  test("the translated strings really are different words", () => {
    expect(buildCatalog("fr").moduleInsights).toBe("Analyses");
    expect(buildCatalog("nl").moduleInsights).toBe("Inzichten");
    expect(buildCatalog("fr").insightsAddChart).toBe("Ajouter un graphique");
    expect(buildCatalog("nl").insightsAddChart).toBe("Grafiek toevoegen");
    expect(buildCatalog("de").insightsAddChart).toBe("Diagramm hinzufügen");
    // A period abbreviation is a translation too: an axis reading "Q1" in
    // French, or "W03" in Dutch, is English leaking onto a chart.
    expect(buildCatalog("fr").insightsQuarter(1, 2026)).toBe("T1 2026");
    expect(buildCatalog("nl").insightsQuarter(1, 2026)).toBe("K1 2026");
    expect(buildCatalog("fr").insightsWeek(3, 2026)).toBe("S3 2026");
    expect(buildCatalog("de").insightsWeek(3, 2026)).toBe("KW 3 2026");
    // …and the plural branch of the unconverted-documents note, which the
    // English catalog builds from two separate sentences.
    expect(buildCatalog("fr").insightsNoteUnconverted(1)).toContain(
      "1 document n’a pas pu",
    );
    expect(buildCatalog("fr").insightsNoteUnconverted(3)).toContain(
      "3 documents n’ont pas pu",
    );
    expect(buildCatalog("nl").insightsNoteUnconverted(1)).toContain(
      "1 document kon niet",
    );
    expect(buildCatalog("nl").insightsNoteUnconverted(3)).toContain(
      "3 documenten konden niet",
    );
  });

  test("the overview's chart titles match the words the server seeds", () => {
    // The Business overview is written by `insights_gallery.rs` in the
    // reader's language, and the gallery offers the same seven charts from
    // this catalog. If the two disagree, pinning a chart a tenant already has
    // looks like a different chart. Keep this list in step with SeedWords.
    expect(buildCatalog("fr").insightsGalleryOutstanding).toBe(
      "Créances en cours",
    );
    expect(buildCatalog("fr").insightsGalleryWonThisMonth).toBe(
      "Gagné ce mois-ci",
    );
    expect(buildCatalog("fr").insightsGalleryRevenueByMonth).toBe(
      "Chiffre d’affaires par mois",
    );
    expect(buildCatalog("fr").insightsGalleryOverdueAging).toBe(
      "Retards par ancienneté",
    );
    expect(buildCatalog("fr").insightsGalleryPipelineByStage).toBe(
      "Pipeline par étape",
    );
    expect(buildCatalog("fr").insightsGalleryVatByQuarter).toBe(
      "TVA par trimestre",
    );
    expect(buildCatalog("fr").insightsGalleryWinRateByQuarter).toBe(
      "Taux de réussite par trimestre",
    );
    expect(buildCatalog("nl").insightsGalleryOutstanding).toBe("Openstaand");
    expect(buildCatalog("nl").insightsGalleryWonThisMonth).toBe(
      "Gewonnen deze maand",
    );
    expect(buildCatalog("nl").insightsGalleryRevenueByMonth).toBe(
      "Omzet per maand",
    );
    expect(buildCatalog("nl").insightsGalleryOverdueAging).toBe(
      "Achterstand per ouderdom",
    );
    expect(buildCatalog("nl").insightsGalleryPipelineByStage).toBe(
      "Pipeline per fase",
    );
    expect(buildCatalog("nl").insightsGalleryVatByQuarter).toBe(
      "Btw per kwartaal",
    );
    expect(buildCatalog("nl").insightsGalleryWinRateByQuarter).toBe(
      "Winstpercentage per kwartaal",
    );
    expect(buildCatalog("de").insightsGalleryOutstanding).toBe(
      "Offene Forderungen",
    );
    expect(buildCatalog("de").insightsGalleryWonThisMonth).toBe(
      "Diesen Monat gewonnen",
    );
    expect(buildCatalog("de").insightsGalleryRevenueByMonth).toBe(
      "Umsatz nach Monat",
    );
    expect(buildCatalog("de").insightsGalleryOverdueAging).toBe(
      "Überfällig nach Alter",
    );
    expect(buildCatalog("de").insightsGalleryPipelineByStage).toBe(
      "Pipeline nach Phase",
    );
    expect(buildCatalog("de").insightsGalleryVatByQuarter).toBe(
      "MwSt. nach Quartal",
    );
    expect(buildCatalog("de").insightsGalleryWinRateByQuarter).toBe(
      "Erfolgsquote nach Quartal",
    );
  });
});

describe("alo Projects is fully translated (B3.11)", () => {
  /** The B3 surface: the Projects tab and its four views, the week grid a
   *  person fills in, the approvals inbox somebody else decides in, the
   *  profitability report, and the agent cards that propose hours. Same rule as
   *  billing, CRM and Insights above — a timesheet an employee is answerable
   *  for, and a manager approves, must not be half in English. */
  const PROJECTS_AGENT_KEYS = [
    "agentActLogTime",
    "agentActProjectStatus",
    "agentActDraftTimesheet",
    "agentFieldProject",
    "agentFieldDay",
    "agentFieldDuration",
    "agentLogTimeNote",
    "agentProjectStatusNote",
    "agentDraftTimesheetNote",
    "agentTimeLogged",
    "agentStatusHours",
    "agentStatusBillable",
    "agentStatusBudget",
    "agentStatusBudgetUsed",
    "agentStatusNoBudget",
    "agentStatusInternal",
    "agentStatusCustomer",
    "agentStatusMilestones",
    "agentStatusMilestonesDone",
    "agentStatusMilestonesLate",
    "agentStatusNoMilestones",
    "agentStatusNext",
    "agentStatusTasks",
    "agentStatusTasksOpen",
    "agentStatusTasksOverdue",
    "agentStatusLastWorked",
    "agentStatusNeverWorked",
    "agentDraftedCount",
    "agentDraftedNone",
    "agentDraftedRange",
    "agentDraftedTotal",
    "agentDraftedOverlap",
    "agentDraftedOverlaps",
    "agentDraftedNote",
    "agentDraftedLeftOut",
    "agentDraftedReason",
  ];
  const projectsKeys = Object.keys(en).filter(
    (key) =>
      key.startsWith("projects") ||
      key === "moduleProjects" ||
      PROJECTS_AGENT_KEYS.includes(key),
  );

  test("the key list is the real Projects surface, not an empty filter", () => {
    expect(projectsKeys.length).toBeGreaterThan(190);
    for (const key of PROJECTS_AGENT_KEYS) expect(en).toHaveProperty(key);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])("%s translates every Projects string", (_locale, catalog) => {
    const missing = projectsKeys.filter((key) => !(key in catalog));
    expect(missing).toEqual([]);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])(
    "%s keeps every interpolation a function of the same shape",
    (locale, catalog) => {
      for (const key of projectsKeys) {
        const source = (en as Record<string, unknown>)[key];
        const translated = (catalog as Record<string, unknown>)[key];
        expect(typeof translated).toBe(typeof source);
        if (typeof source === "function" && typeof translated === "function") {
          expect(translated.length).toBe(source.length);
        } else {
          expect(String(translated).trim()).not.toBe("");
        }
      }
      expect(locale).toMatch(/^(fr|nl|de)$/);
    },
  );

  test("the translated strings really are different words", () => {
    expect(buildCatalog("fr").moduleProjects).toBe("Projets");
    expect(buildCatalog("nl").moduleProjects).toBe("Projecten");
    expect(buildCatalog("de").moduleProjects).toBe("Projekte");
    expect(buildCatalog("fr").projectsMilestoneReached).toBe("Atteint");
    expect(buildCatalog("nl").projectsMilestoneReached).toBe("Bereikt");
    expect(buildCatalog("de").projectsMilestoneReached).toBe("Erreicht");
    expect(buildCatalog("fr").projectsWeekRejected).toBe("Renvoyée");
    expect(buildCatalog("nl").projectsWeekRejected).toBe("Teruggestuurd");
    expect(buildCatalog("de").projectsWeekRejected).toBe("Zurückgewiesen");
    // …including the ones built by a function, and both branches of a plural.
    expect(buildCatalog("fr").projectsSuggestionsWaiting(1)).toContain(
      "1 proposition",
    );
    expect(buildCatalog("fr").projectsSuggestionsWaiting(3)).toContain(
      "3 propositions",
    );
    expect(buildCatalog("nl").projectsSuggestionsWaiting(1)).toContain(
      "1 voorstel wacht",
    );
    expect(buildCatalog("nl").projectsSuggestionsWaiting(3)).toContain(
      "3 voorstellen wachten",
    );
    expect(buildCatalog("de").projectsSuggestionsWaiting(1)).toContain(
      "1 Vorschlag wartet",
    );
    expect(buildCatalog("de").projectsSuggestionsWaiting(3)).toContain(
      "3 Vorschläge warten",
    );
  });

  test("a duration is written in the reader's own units", () => {
    // The hour and minute letters are the easiest thing on this surface to
    // leave in English: they look like punctuation. "7h 30m" on a French
    // timesheet is English leaking onto the one number an employee signs for.
    expect(buildCatalog("fr").projectsHoursShort(7)).toBe("7 h");
    expect(buildCatalog("fr").projectsMinutesShort(30)).toBe("30 min");
    expect(buildCatalog("nl").projectsHoursShort(7)).toBe("7 u");
    expect(buildCatalog("nl").projectsMinutesShort(30)).toBe("30 min");
    expect(buildCatalog("de").projectsHoursShort(7)).toBe("7 Std.");
    expect(buildCatalog("de").projectsMinutesShort(30)).toBe("30 Min.");
    // French puts a space before the percent sign (as does German, per
    // DIN 5008); Dutch does not.
    expect(buildCatalog("fr").projectsPercent(60)).toBe("60 %");
    expect(buildCatalog("nl").projectsPercent(60)).toBe("60%");
    expect(buildCatalog("de").projectsPercent(60)).toBe("60 %");
  });

  test("every reason a meeting was left out has words in each language", () => {
    // The server sends reason codes, so an untranslated branch here is a
    // French card that explains itself in English. The default branch matters
    // most: a newer server's reason must still read as "left out".
    const codes = [
      "allDay",
      "alreadyDrafted",
      "noDuration",
      "tooLong",
      "weekLocked",
      "limitReached",
      "outsideRange",
      "somethingNewerServersKnow",
    ];
    for (const locale of ["fr", "nl", "de"] as const) {
      const say = buildCatalog(locale).agentDraftedReason;
      for (const code of codes) {
        expect(say(code)).not.toBe(en.agentDraftedReason(code));
        expect(say(code).trim()).not.toBe("");
      }
    }
    expect(buildCatalog("fr").agentDraftedReason("weekLocked")).toBe(
      "cette semaine est soumise",
    );
    expect(buildCatalog("nl").agentDraftedReason("weekLocked")).toBe(
      "die week is ingediend",
    );
    expect(buildCatalog("de").agentDraftedReason("weekLocked")).toBe(
      "diese Woche ist eingereicht",
    );
  });
});

describe("alo Finance is fully translated (B4.15)", () => {
  /** The B4 surface: expense claims an employee is answerable for, the bank
   *  statements and the pile they leave, the chart of accounts, the four
   *  reports a company files from, and the three agent cards that read the
   *  books. Same rule as billing, CRM, Insights and Projects above — a figure
   *  somebody copies into a VAT return must not come with half its words in
   *  English. */
  const FINANCE_AGENT_KEYS = Object.keys(en).filter(
    (key) =>
      key.startsWith("agentCategorise") ||
      key.startsWith("agentVat") ||
      key.startsWith("agentAnomaly") ||
      key === "agentActCategorise" ||
      key === "agentActVatSummary" ||
      key === "agentActFlagAnomalies",
  );
  const financeKeys = Object.keys(en).filter(
    (key) =>
      key.startsWith("finance") ||
      key === "moduleFinance" ||
      FINANCE_AGENT_KEYS.includes(key),
  );

  test("the key list is the real Finance surface, not an empty filter", () => {
    // A typo in the filter above would make every assertion below vacuous.
    expect(financeKeys.length).toBeGreaterThan(300);
    expect(FINANCE_AGENT_KEYS.length).toBeGreaterThan(40);
    expect(en).toHaveProperty("moduleFinance");
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])("%s translates every Finance string", (_locale, catalog) => {
    const missing = financeKeys.filter((key) => !(key in catalog));
    expect(missing).toEqual([]);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])(
    "%s keeps every interpolation a function of the same shape",
    (locale, catalog) => {
      for (const key of financeKeys) {
        const source = (en as Record<string, unknown>)[key];
        const translated = (catalog as Record<string, unknown>)[key];
        expect(typeof translated).toBe(typeof source);
        if (typeof source === "function" && typeof translated === "function") {
          expect(translated.length).toBe(source.length);
        } else {
          expect(String(translated).trim()).not.toBe("");
        }
      }
      expect(locale).toMatch(/^(fr|nl|de)$/);
    },
  );

  test("the translated strings really are different words", () => {
    expect(buildCatalog("fr").moduleFinance).toBe("Finance");
    expect(buildCatalog("nl").moduleFinance).toBe("Financiën");
    expect(buildCatalog("de").moduleFinance).toBe("Finanzen");
    // The four report titles are the words an accountant looks for.
    expect(buildCatalog("fr").financeReportPl).toBe("Compte de résultat");
    expect(buildCatalog("nl").financeReportPl).toBe("Winst-en-verliesrekening");
    expect(buildCatalog("de").financeReportPl).toBe(
      "Gewinn- und Verlustrechnung",
    );
    expect(buildCatalog("fr").financeReportVat).toBe("Déclaration de TVA");
    expect(buildCatalog("nl").financeReportVat).toBe("Btw-aangifte");
    expect(buildCatalog("de").financeReportVat).toBe("MwSt.-Erklärung");
    expect(buildCatalog("fr").financeChartLoadFailed).toContain(
      "plan comptable",
    );
    expect(buildCatalog("nl").financeChartLoadFailed).toContain(
      "rekeningschema",
    );
    expect(buildCatalog("de").financeChartLoadFailed).toContain("Kontenplan");
    // …including the ones built by a function, and both branches of a plural.
    expect(buildCatalog("fr").financeReportOpenDocuments(1)).toBe(
      "1 document ouvert",
    );
    expect(buildCatalog("fr").financeReportOpenDocuments(3)).toBe(
      "3 documents ouverts",
    );
    expect(buildCatalog("nl").financeReportOpenDocuments(1)).toBe(
      "1 openstaand document",
    );
    expect(buildCatalog("nl").financeReportOpenDocuments(3)).toBe(
      "3 openstaande documenten",
    );
    expect(buildCatalog("de").financeReportOpenDocuments(1)).toBe(
      "1 offenes Dokument",
    );
    expect(buildCatalog("de").financeReportOpenDocuments(3)).toBe(
      "3 offene Dokumente",
    );
  });

  test("no sentence makes a participle agree with an interpolated amount", () => {
    // French agreement is the trap on this surface: an amount arrives as a
    // formatted string, so "1,00 € restent dus" would be ungrammatical for
    // every singular amount. Each of these is deliberately invariable.
    expect(buildCatalog("fr").financeBankStillOwedIs("1,00 €")).toBe(
      "1,00 € restant à payer",
    );
    expect(buildCatalog("fr").financeBankPickSubtitle("1,00 €")).toContain(
      "Nous avons reçu 1,00 €",
    );
    expect(buildCatalog("fr").financeReportUnbalanced("1,00 €")).toContain(
      "un écart de 1,00 € n’est pas expliqué",
    );
    expect(
      buildCatalog("fr").financeMarkPaidBackSubtitle("Marie", "1,00 €"),
    ).toBe("Retour de 1,00 € à Marie.");
  });

  test("the words the two modules share stay one word", () => {
    // A payment settles a *billing* invoice and is read on a *finance* screen.
    // If "issued" is one word under Billing and another under Finance, the
    // same document appears to have two states.
    expect(buildCatalog("fr").billingStatusIssued).toBe("Émise");
    expect(buildCatalog("fr").financeBankNoOpenInvoices).toContain(
      "facture émise",
    );
    expect(buildCatalog("nl").billingStatusIssued).toBe("Uitgegeven");
    expect(buildCatalog("nl").financeBankNoOpenInvoices).toContain(
      "uitgegeven factuur",
    );
    expect(buildCatalog("nl").financeReportAgedEmptyBody).toContain(
      "uitgegeven document",
    );
    expect(buildCatalog("de").billingStatusIssued).toBe("Ausgestellt");
    expect(buildCatalog("de").financeBankNoOpenInvoices).toContain(
      "ausgestellte Rechnung",
    );
    expect(buildCatalog("de").financeReportAgedEmptyBody).toContain(
      "ausgestellte Dokument",
    );
  });

  test("every reason and kind the agent cards render has words in each language", () => {
    // The server sends codes, so an untranslated branch is a French card that
    // explains itself in English. The default branch matters most: a newer
    // server's code must still read as an answer.
    const reasons = [
      "noMerchant",
      "noHistory",
      "alreadyProposed",
      "declined",
      "somethingNewerServersKnow",
    ];
    const kinds = [
      "duplicate",
      "unusualAmount",
      "missingRecurring",
      "somethingNewerServersKnow",
    ];
    for (const locale of ["fr", "nl", "de"] as const) {
      const say = buildCatalog(locale);
      for (const reason of reasons) {
        expect(say.agentCategoriseReason(reason)).not.toBe(
          en.agentCategoriseReason(reason),
        );
        expect(say.agentCategoriseReason(reason).trim()).not.toBe("");
      }
      for (const kind of kinds) {
        expect(say.agentAnomalyKind(kind)).not.toBe(en.agentAnomalyKind(kind));
        expect(say.agentAnomalyKind(kind).trim()).not.toBe("");
      }
    }
    expect(buildCatalog("fr").agentAnomalyKind("duplicate")).toBe(
      "Comptabilisé deux fois en une semaine",
    );
    expect(buildCatalog("nl").agentAnomalyKind("duplicate")).toBe(
      "Twee keer geboekt in één week",
    );
    expect(buildCatalog("de").agentAnomalyKind("duplicate")).toBe(
      "Zweimal in einer Woche gebucht",
    );
  });
});

describe("alo Inventory is fully translated (B5.11)", () => {
  /** The B5 surface: the catalog, the stock list and the movement history, the
   *  purchase and sales orders with the sentences that precede an irreversible
   *  act, the barcode scanner, and the two agent cards that propose orders.
   *  Same rule as billing, CRM, Insights, Projects and Finance above — a
   *  sentence that says "this draws a number and freezes the order" must not be
   *  the one thing on the screen left in English. */
  const INVENTORY_AGENT_KEYS = Object.keys(en).filter(
    (key) =>
      key.startsWith("agentReorder") ||
      key.startsWith("agentStock") ||
      key === "agentActReorderProposals" ||
      key === "agentActStockAnswer" ||
      key === "agentFieldSupplier" ||
      key === "agentFieldLocation" ||
      key === "agentFieldProduct",
  );
  const inventoryKeys = Object.keys(en).filter(
    (key) =>
      key.startsWith("inventory") ||
      key === "moduleInventory" ||
      INVENTORY_AGENT_KEYS.includes(key),
  );

  test("the key list is the real Inventory surface, not an empty filter", () => {
    // A typo in the filter above would make every assertion below vacuous.
    expect(inventoryKeys.length).toBeGreaterThan(240);
    expect(INVENTORY_AGENT_KEYS.length).toBeGreaterThan(20);
    expect(en).toHaveProperty("moduleInventory");
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])("%s translates every Inventory string", (_locale, catalog) => {
    const missing = inventoryKeys.filter((key) => !(key in catalog));
    expect(missing).toEqual([]);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])(
    "%s keeps every interpolation a function of the same shape",
    (locale, catalog) => {
      for (const key of inventoryKeys) {
        const source = (en as Record<string, unknown>)[key];
        const translated = (catalog as Record<string, unknown>)[key];
        expect(typeof translated).toBe(typeof source);
        if (typeof source === "function" && typeof translated === "function") {
          expect(translated.length).toBe(source.length);
        } else {
          expect(String(translated).trim()).not.toBe("");
        }
      }
      expect(locale).toMatch(/^(fr|nl|de)$/);
    },
  );

  test("the translated strings really are different words", () => {
    expect(buildCatalog("fr").moduleInventory).toBe("Inventaire");
    expect(buildCatalog("nl").moduleInventory).toBe("Voorraad");
    expect(buildCatalog("de").moduleInventory).toBe("Lager");
    // The two order documents are what a stranger reads.
    expect(buildCatalog("fr").inventoryNewPurchaseOrder).toBe(
      "Nouvelle commande d’achat",
    );
    expect(buildCatalog("nl").inventoryNewPurchaseOrder).toBe(
      "Nieuwe inkooporder",
    );
    expect(buildCatalog("fr").inventoryPoStatusSent).toBe("Passée");
    expect(buildCatalog("nl").inventoryPoStatusSent).toBe("Geplaatst");
    // …including the ones built by a function.
    expect(buildCatalog("fr").inventoryArrivalNo(2)).toBe("Arrivée 2");
    expect(buildCatalog("nl").inventoryConsignmentNo(2)).toBe("Zending 2");
    expect(buildCatalog("fr").agentReorderDrafted(1)).toBe(
      "1 commande en brouillon",
    );
    expect(buildCatalog("fr").agentReorderDrafted(3)).toBe(
      "3 commandes en brouillon",
    );
    expect(buildCatalog("nl").agentReorderDrafted(1)).toBe("1 conceptorder");
    expect(buildCatalog("nl").agentReorderDrafted(3)).toBe("3 conceptorders");
    expect(buildCatalog("de").inventoryConsignmentNo(2)).toBe("Sendung 2");
    expect(buildCatalog("de").inventoryArrivalNo(2)).toBe("Eingang 2");
  });

  test("German splits the trade's two documents and keeps the warehouse's own nouns", () => {
    // English says "order" for both directions; German paperwork does not: a
    // purchase order is a Bestellung, a sales order an Auftrag, and where one
    // column lists both the document is a Beleg. The movement reasons are the
    // warehouse's own words (Wareneingang, Umlagerung, Inventur, Schwund) —
    // the same finding as nl's ingeslagen/uitgeslagen: written, not
    // translated.
    const catalog = buildCatalog("de");
    expect(catalog.inventoryTabPurchasing).toBe("Einkauf");
    expect(catalog.inventoryTabSales).toBe("Aufträge");
    expect(catalog.inventoryColOrder).toBe("Beleg");
    expect(catalog.inventoryReasonTransfer).toBe("Umlagerung");
    expect(catalog.inventoryReceiveWhere).toBe("Eingelagert in");
    expect(catalog.inventoryDeliverWhere).toBe("Entnommen aus");
    // The agent card's quantity phrase is invariable, as in French: the
    // number arrives formatted, so no word may have to agree with it.
    expect(catalog.agentReorderNeeded("1", "Stück")).toBe("1 Stück benötigt");
    expect(catalog.agentReorderNeeded("12", "")).toBe("12 benötigt");
  });

  test("no French label makes a participle agree with goods it cannot see", () => {
    // A movement reason and an adjustment reason describe *goods* whose gender
    // the sentence never learns, so every one of them is a noun. A status, by
    // contrast, always has "la commande" for a subject and agrees on purpose —
    // the same split B4.15 found between Finance's sentences and its statuses.
    for (const key of [
      "inventoryReasonReceipt",
      "inventoryReasonDelivery",
      "inventoryReasonTransfer",
      "inventoryReasonAdjustment",
      "inventoryReasonReturn",
      "inventoryReasonShrinkage",
      "inventoryReasonCount",
      "inventoryAdjustDamaged",
      "inventoryAdjustLost",
      "inventoryAdjustFound",
      "inventoryAdjustExpired",
    ] as const) {
      expect(buildCatalog("fr")[key]).not.toMatch(/(é|ée|és|ées)$/);
    }
    expect(buildCatalog("fr").inventoryAdjustDamaged).toBe("Casse");
    expect(buildCatalog("fr").inventoryPoStatusReceived).toBe("Reçue");
    expect(buildCatalog("fr").inventorySoStatusDelivered).toBe("Livrée");
    // And the quantity an agent card asks for is invariable, because the
    // number arrives already formatted: "1 pièce nécessaire" cannot agree.
    expect(buildCatalog("fr").agentReorderNeeded("1", "pièce")).toBe(
      "1 pièce à commander",
    );
    expect(buildCatalog("fr").agentReorderNeeded("12", "")).toBe(
      "12 à commander",
    );
  });

  test("Dutch uses the warehouse's own verbs, not loanwords", () => {
    // Goods are *ingeslagen* and *uitgeslagen*. "Gepickt" on the one screen a
    // warehouse worker opens all day is the tell that a product was translated
    // rather than written — the same finding as B4.15's "afletteren".
    expect(buildCatalog("nl").inventoryReceiveWhere).toBe("Ingeslagen op");
    expect(buildCatalog("nl").inventoryDeliverWhere).toBe("Uitgeslagen van");
    expect(buildCatalog("nl").inventoryReasonShrinkage).toBe("Derving");
  });

  test("the words Inventory shares with Billing stay one word", () => {
    // A sales order raises a *billing* invoice and a receipt raises a *billing*
    // bill. If a draft is one word here and another there, the same document
    // appears to have two states.
    expect(buildCatalog("fr").inventoryInvoiceDrafted).toContain("brouillon");
    expect(buildCatalog("fr").inventoryInvoiceDrafted).toContain(
      buildCatalog("fr").moduleBilling,
    );
    expect(buildCatalog("nl").inventoryDraftInvoice).toBe("Conceptfactuur");
    expect(buildCatalog("nl").crmDocumentDraft("invoice")).toBe(
      "conceptfactuur",
    );
    expect(buildCatalog("nl").inventoryInvoiceDrafted).toContain(
      buildCatalog("nl").moduleBilling,
    );
    expect(buildCatalog("de").inventoryDraftInvoice).toBe("Rechnungsentwurf");
    expect(buildCatalog("de").crmDocumentDraft("invoice")).toBe(
      "Rechnungsentwurf",
    );
    expect(buildCatalog("de").inventoryInvoiceDrafted).toContain(
      buildCatalog("de").moduleBilling,
    );
  });

  test("every reason a product was left out of an order has words in each language", () => {
    // The server sends reason codes, so an untranslated branch is a French card
    // that explains itself in English. The default branch matters most: a newer
    // server's reason must still read as "left out", never as nothing.
    const codes = ["noSupplier", "nothingToBuy", "somethingNewerServersKnow"];
    for (const locale of ["fr", "nl", "de"] as const) {
      const say = buildCatalog(locale).agentReorderReason;
      for (const code of codes) {
        expect(say(code)).not.toBe(en.agentReorderReason(code));
        expect(say(code).trim()).not.toBe("");
      }
    }
    expect(buildCatalog("fr").agentReorderReason("noSupplier")).toBe(
      "personne ne vous l’a chiffré",
    );
    expect(buildCatalog("nl").agentReorderReason("noSupplier")).toBe(
      "niemand heeft er u een prijs voor gegeven",
    );
    expect(buildCatalog("de").agentReorderReason("noSupplier")).toBe(
      "niemand hat Ihnen dafür einen Preis genannt",
    );
  });
});

describe("alo HR is fully translated (B6.11)", () => {
  /** The B6 surface: the staff directory and the org chart, the leave a person
   *  asks for and a manager decides, the month that says who is away, the
   *  hiring board a candidate is moved across, the approvals inbox that
   *  gathers three queues, and the one agent card that reads absences.
   *
   *  The ratchet below checks that a key *exists* in all three catalogs and
   *  lets `UNTRANSLATED` exempt one. This describe exists because no HR key
   *  may ever take that exemption: these strings are read by a person about
   *  their own employment — the day they asked for, the answer they were
   *  given, the record their employer keeps — and a half-English sentence
   *  there is not a cosmetic gap. It also pins arity, non-emptiness, and the
   *  four vocabulary decisions below, none of which the ratchet can see. */
  const HR_AGENT_KEYS = Object.keys(en).filter(
    (key) => key.startsWith("agentWhoIsOff") || key === "agentActWhoIsOff",
  );
  const hrKeys = Object.keys(en).filter(
    (key) =>
      /^hr[A-Z]/.test(key) ||
      key === "moduleHr" ||
      HR_AGENT_KEYS.includes(key),
  );

  test("the key list is the real HR surface, not an empty filter", () => {
    // A typo in the filter above would make every assertion below vacuous.
    expect(hrKeys.length).toBeGreaterThan(190);
    expect(HR_AGENT_KEYS.length).toBeGreaterThan(6);
    expect(en).toHaveProperty("moduleHr");
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])("%s translates every HR string", (_locale, catalog) => {
    const missing = hrKeys.filter((key) => !(key in catalog));
    expect(missing).toEqual([]);
  });

  test("no HR key may be exempted from translation", () => {
    // The ratchet's escape hatch is deliberately unavailable here: a Dutch
    // employee reading their own leave balance in English is the failure this
    // whole wave is about.
    expect(hrKeys.filter((key) => UNTRANSLATED.includes(key))).toEqual([]);
  });

  test.each([
    ["fr", fr],
    ["nl", nl],
    ["de", de],
  ])(
    "%s keeps every interpolation a function of the same shape",
    (locale, catalog) => {
      for (const key of hrKeys) {
        const source = (en as Record<string, unknown>)[key];
        const translated = (catalog as Record<string, unknown>)[key];
        expect(typeof translated).toBe(typeof source);
        if (typeof source === "function" && typeof translated === "function") {
          expect(translated.length).toBe(source.length);
        } else {
          expect(String(translated).trim()).not.toBe("");
        }
      }
      expect(locale).toMatch(/^(fr|nl|de)$/);
    },
  );

  test("the translated strings really are different words", () => {
    expect(buildCatalog("fr").moduleHr).toBe("Personnes");
    expect(buildCatalog("nl").moduleHr).toBe("Mensen");
    expect(buildCatalog("de").moduleHr).toBe("Personen");
    // …including the ones built by a function, in both branches.
    expect(buildCatalog("fr").hrWorkingDays(1)).toBe("1 jour");
    expect(buildCatalog("fr").hrWorkingDays(4)).toBe("4 jours");
    expect(buildCatalog("nl").hrPeopleCount(1)).toBe("1 persoon");
    expect(buildCatalog("nl").hrPeopleCount(9)).toBe("9 mensen");
    expect(buildCatalog("fr").agentWhoIsOffCount(1)).toBe("1 personne");
    expect(buildCatalog("nl").agentWhoIsOffDays(3)).toBe("3 dagen");
    expect(buildCatalog("de").agentWhoIsOffDays(1)).toBe("1 Tag");
    expect(buildCatalog("de").agentWhoIsOffDays(3)).toBe("3 Tage");
  });

  test("no string in any language says WHY somebody is away", () => {
    // The design note's EU AI Act and health-data posture: the absence layer
    // carries names and days and no reason at all, and translating it is
    // exactly where a reason would creep back in — a Dutch "ziekteverlof" on
    // a screen whose English says only "away" would invent health data the
    // server never sent. Public holidays are a different thing and stay.
    const reason =
      /\bsick|\billness|\bmalad|\bziek|\bmatern|\bparental\b|ouderschap|zwangerschap|krank|elternzeit|mutterschutz|schwanger/i;
    for (const catalog of [en, fr, nl, de]) {
      const named = hrKeys
        .filter((key) => key in catalog)
        .filter((key) => {
          const value = (catalog as Record<string, unknown>)[key];
          return typeof value === "string" && reason.test(value);
        });
      expect(named).toEqual([]);
    }
  });

  test("French names an employment by its term, and Dutch by the trade's own word", () => {
    // A French contract is a *durée indéterminée* or a *durée déterminée* —
    // that is what the paper itself says, and "Permanent" is a translation of
    // our English rather than the name of the thing. In Dutch a self-employed
    // person is a *zelfstandige*, never an *aannemer*, which is a builder.
    expect(buildCatalog("fr").hrKindPermanent).toBe("Durée indéterminée");
    expect(buildCatalog("fr").hrKindFixedTerm).toBe("Durée déterminée");
    expect(buildCatalog("nl").hrKindPermanent).toBe("Vast");
    expect(buildCatalog("nl").hrKindContractor).toBe("Zelfstandige");
    // German paperwork says unbefristet/befristet, and a self-employed
    // person is selbstständig — never "Auftragnehmer", which is a party to
    // a contract, not a way of working.
    expect(buildCatalog("de").hrKindPermanent).toBe("Unbefristet");
    expect(buildCatalog("de").hrKindFixedTerm).toBe("Befristet");
    expect(buildCatalog("de").hrKindContractor).toBe("Selbstständig");
  });

  test("a candidate who did not get the job is told so without an insult", () => {
    // *Non retenu* is what a French rejection letter says; *Rejeté* is what a
    // form says about a row. Dutch takes the same care: *niet verder*, the
    // process ending, rather than *afgewezen* aimed at the person.
    expect(buildCatalog("fr").hrStageRejected).toBe("Non retenu");
    expect(buildCatalog("nl").hrStageRejected).toBe("Niet verder");
    // German takes the rejection letter's own phrase — nicht berücksichtigt —
    // never the form's Zurückgewiesen or Abgelehnt aimed at the person.
    expect(buildCatalog("de").hrStageRejected).toBe("Nicht berücksichtigt");
  });

  test("having left is said plainly, in every language", () => {
    // No euphemism: not "transitioned", not "no longer with us". A record
    // that a person's employment ended is a fact the directory states.
    expect(buildCatalog("fr").hrLeft).toBe("Parti");
    expect(buildCatalog("nl").hrLeft).toBe("Vertrokken");
    expect(buildCatalog("de").hrLeft).toBe("Ausgeschieden");
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

describe("the ratchet: no new key drifts out of Dutch, French or German", () => {
  // The catalog falls back to English for a missing key, which is right at
  // runtime — a blank screen would be worse — and is exactly why 588 keys
  // drifted without anyone noticing. Nothing surfaces an untranslated string
  // except a Dutch user reading English.
  //
  // So the check is here instead. `UNTRANSLATED` is the debt as it stood when
  // this was written; anything outside it must exist in all four languages.
  // German joined when M4.1 completed its catalog (tranche 9).
  const nlKeys = new Set(Object.keys(nl));
  const frKeys = new Set(Object.keys(fr));
  const deKeys = new Set(Object.keys(de));
  const allowed = new Set(UNTRANSLATED);

  test("a new English key is translated, or the build says which is not", () => {
    const drifted = Object.keys(en).filter(
      (key) =>
        !allowed.has(key) &&
        (!nlKeys.has(key) || !frKeys.has(key) || !deKeys.has(key)),
    );
    expect(
      drifted,
      `These keys need Dutch, French and German, or an explicit line in untranslated.ts:\n  ${drifted.join("\n  ")}`,
    ).toEqual([]);
  });

  test("the list only shrinks — a translated key must be removed from it", () => {
    // Otherwise the debt list quietly becomes a permanent exemption, and the
    // number stops meaning anything.
    const stale = UNTRANSLATED.filter(
      (key) => nlKeys.has(key) && frKeys.has(key) && deKeys.has(key),
    );
    expect(
      stale,
      `Translated now — delete these lines from untranslated.ts:\n  ${stale.join("\n  ")}`,
    ).toEqual([]);
  });

  test("the list names only keys that exist", () => {
    // A renamed or deleted key leaves a line behind that would silently
    // exempt nothing, and the count would overstate the debt.
    const gone = UNTRANSLATED.filter((key) => !(key in en));
    expect(
      gone,
      `No longer in en.ts — remove:\n  ${gone.join("\n  ")}`,
    ).toEqual([]);
  });
});
