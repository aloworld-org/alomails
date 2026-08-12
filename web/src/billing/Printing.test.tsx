// What printing and the issuer identity promise (B1.16), proven against a
// recorded network: that the printed page is *fetched* from the server with
// the session's credentials rather than composed here, that it reaches the
// browser's print dialog without leaving the app, and that the details it is
// made of are edited under the module's own three rules.
//
// Only the network — and `window.print`, which no headless DOM implements — is
// fake. The real router, the real module routes, the real client, the real
// editor shell and the real settings form all run.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { getLocale, strings } from "../i18n";
import { BillingModule } from "./BillingModule";
import type { BillingCustomer, BillingInvoice, BillingSettings } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

interface Reply {
  match: (url: string, method: string) => boolean;
  status: number;
  body: unknown;
  /** Sent as `text/html` rather than JSON — what the print route answers. */
  html?: string;
}

const calls: Call[] = [];
let replies: Reply[] = [];

function reply(urlPart: string, method: string, body: unknown, status = 200) {
  replies.push({ match: (url, m) => url.includes(urlPart) && m === method, status, body });
}

function replyHtml(urlPart: string, html: string) {
  replies.push({ match: (url, m) => url.includes(urlPart) && m === "GET", status: 200, body: {}, html });
}

const CUSTOMER: BillingCustomer = {
  id: "c-1",
  name: "Acme GmbH",
  addressLine1: "Hauptstrasse 1",
  addressLine2: "",
  postalCode: "20095",
  city: "Hamburg",
  country: "DE",
  vatId: "DE811907980",
  email: "billing@acme.test",
  paymentTermsDays: 14,
  currency: "EUR",
  contactId: null,
  archived: false,
  archivedAt: null,
  createdBy: "u-1",
  createdAt: "2026-08-06T10:00:00Z",
  updatedAt: "2026-08-06T10:00:00Z",
};

const ISSUED: BillingInvoice = {
  id: "inv-1",
  customerId: "c-1",
  status: "issued",
  currency: "EUR",
  number: "INV-2026-00001",
  issueDate: "2026-08-06",
  dueDate: "2026-08-20",
  paymentTermsDays: 14,
  overdue: false,
  creditNote: false,
  creditsInvoiceId: null,
  quoteId: null,
  scheduleId: null,
  scheduleDueDate: null,
  reference: "PO-42",
  note: "",
  createdBy: "u-1",
  createdAt: "2026-08-06T10:00:00Z",
  updatedAt: "2026-08-06T10:00:00Z",
  lines: [
    {
      id: "l-1",
      description: "Consulting",
      unit: "hour",
      qtyMilli: 1500,
      unitPriceCents: 12500,
      vatRateBp: 2100,
      netCents: 18750,
    },
  ],
  totals: {
    netCents: 18750,
    vatCents: 3938,
    grossCents: 22688,
    vatByRate: [{ rateBp: 2100, netCents: 18750, vatCents: 3938 }],
  },
  // Issued in the tenant's own currency, so the identity rate and nothing to
  // restate on the paper.
  fx: { baseCurrency: "EUR", rateMicro: 1_000_000, rate: "1.0", rateDate: "2026-08-06" },
  // Nothing received: the settlement the server sends for a document nobody
  // has paid anything against.
  settlement: {
    grossCents: 22688,
    paidCents: 0,
    outstandingCents: 22688,
    state: "unpaid",
  },
};

/** A tenant that has never saved: the blanks, and `stated: false`. */
const UNSTATED: BillingSettings = {
  stated: false,
  // Never blank, even unstated: a tenant keeps books in something.
  baseCurrency: "EUR",
  legalName: "",
  addressLine1: "",
  addressLine2: "",
  postalCode: "",
  city: "",
  country: "",
  vatId: null,
  registrationNo: "",
  email: "",
  phone: "",
  website: "",
  iban: null,
  bic: null,
  bankName: "",
  accountHolder: "",
  footerNote: "",
  updatedBy: null,
  updatedAt: null,
};

const STATED: BillingSettings = {
  ...UNSTATED,
  stated: true,
  legalName: "Alo Werkplaats B.V.",
  addressLine1: "Keizersgracht 1",
  postalCode: "1015 CJ",
  city: "Amsterdam",
  country: "NL",
  vatId: "NL812345678B01",
  iban: "NL91ABNA0417164300",
  bic: "ABNANL2A",
  updatedBy: "u-1",
  updatedAt: "2026-08-06T10:00:00Z",
};

const SHEET = "<!doctype html><html lang=\"en\"><head><title>Invoice INV-2026-00001</title></head><body>the document</body></html>";

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const index = replies.findIndex((r) => r.match(url, method));
  const answer = index === -1 ? fallback(url, method) : (replies.splice(index, 1)[0] as Reply);
  if (answer.html !== undefined) {
    return new Response(answer.html, {
      status: answer.status,
      headers: { "content-type": "text/html; charset=utf-8" },
    });
  }
  return new Response(JSON.stringify(answer.body), {
    status: answer.status,
    headers: { "content-type": "application/json" },
  });
});

function fallback(url: string, method: string): Reply {
  const body =
    method !== "GET"
      ? {}
      : url.includes("/billing/customers")
        ? { customers: [CUSTOMER] }
        : url.includes("/billing/products")
          ? { products: [] }
          : url.includes("/billing/fx/rates")
            ? { rates: [] }
            : url.includes("/billing/settings")
              ? { settings: UNSTATED }
              : { invoices: [] };
  return { match: () => true, status: 200, body };
}

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

function ui(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DialogProvider>
        <Routes>
          <Route path="/billing/*" element={<BillingModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

function lastWrite(): Call | undefined {
  return calls.filter((c) => c.method !== "GET").at(-1);
}

/** `window.print` exists in no headless DOM, and an iframe's `contentWindow`
 *  never runs a load event here. This stands in for the browser: it records
 *  what was handed to the print dialog and lets the frame's `load` fire. */
function stubPrintableFrames() {
  const printed: string[] = [];
  const real = Object.getOwnPropertyDescriptor(HTMLIFrameElement.prototype, "contentWindow");
  Object.defineProperty(HTMLIFrameElement.prototype, "contentWindow", {
    configurable: true,
    get(this: HTMLIFrameElement) {
      return {
        focus: () => undefined,
        addEventListener: () => undefined,
        print: () => printed.push(this.srcdoc),
      };
    },
  });
  return {
    printed,
    restore: () => {
      if (real === undefined) return;
      Object.defineProperty(HTMLIFrameElement.prototype, "contentWindow", real);
    },
  };
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
  // `printSheet` mounts its frame on `document.body`, outside React, and lets
  // the browser's `afterprint` take it down again — an event no headless DOM
  // fires. Clear any left over from a previous test.
  for (const frame of document.querySelectorAll("iframe")) frame.remove();
});

afterEach(cleanup);

describe("printing a document", () => {
  test("fetches the server's page with the session's fetch and hands it to the print dialog", async () => {
    const frames = stubPrintableFrames();
    try {
      reply("/billing/invoices/inv-1", "GET", { invoice: ISSUED, creditNotes: [] });
      replyHtml("/billing/invoices/inv-1/print", SHEET);
      ui("/billing/invoices/inv-1");

      const button = await screen.findByRole("button", { name: strings.billingPrint });
      fireEvent.click(button);

      // It asked the server for the page — the client renders no document —
      // and said which language to write it in (B1.27): the server holds the
      // document's own words, so the request carries the interface language
      // rather than the client translating anything.
      await waitFor(() =>
        expect(
          calls.some((c) => c.url.endsWith(`/billing/invoices/inv-1/print?lang=${getLocale()}`)),
        ).toBe(true),
      );
      // …through the authorized fetch, which is the whole reason it is not a
      // link: an anonymous tab would get a 401.
      expect(fakeFetch).toHaveBeenCalled();

      const frame = document.querySelector("iframe");
      expect(frame).toBeTruthy();
      // The page is mounted with `srcdoc`, not a blob URL: our own CSP is
      // `frame-src 'self'`, and `blob:` is not `'self'`.
      expect(frame?.getAttribute("srcdoc")).toBe(SHEET);
      expect(frame?.getAttribute("src")).toBeNull();
      // …and sandboxed WITHOUT `allow-scripts`, which is what makes the page
      // inert on this path: a srcdoc document is same-origin with the app and
      // never sees the print response's own Content-Security-Policy.
      const sandbox = frame?.getAttribute("sandbox") ?? "";
      expect(sandbox.split(" ")).not.toContain("allow-scripts");
      expect(sandbox.split(" ")).toContain("allow-same-origin");

      // The frame's own load event does the printing; nothing here fires it.
      await waitFor(() => expect(frames.printed).toEqual([SHEET]));
    } finally {
      frames.restore();
    }
  });

  test("says so in the server's words when the page cannot be prepared, and prints nothing", async () => {
    const frames = stubPrintableFrames();
    try {
      reply("/billing/invoices/inv-1", "GET", { invoice: ISSUED, creditNotes: [] });
      reply("/billing/invoices/inv-1/print", "GET", { detail: "no such customer" }, 404);
      ui("/billing/invoices/inv-1");

      fireEvent.click(await screen.findByRole("button", { name: strings.billingPrint }));

      expect(await screen.findByText("no such customer")).toBeTruthy();
      expect(document.querySelector("iframe")).toBeNull();
      expect(frames.printed).toEqual([]);
    } finally {
      frames.restore();
    }
  });

  test("is not offered on a document that does not exist yet", async () => {
    ui("/billing/invoices/new");
    await screen.findByText(strings.billingNewInvoice);
    expect(screen.queryByRole("button", { name: strings.billingPrint })).toBeNull();
  });
});

describe("the issuer details", () => {
  test("a tenant that has never saved gets the form, not an error", async () => {
    reply("/billing/settings", "GET", { settings: UNSTATED });
    ui("/billing/settings");

    expect(await screen.findByText(strings.billingSettingsFirstRun)).toBeTruthy();
    // Nothing is prefilled and nothing is invented.
    // Anchored at the start because `Field` puts the hint inside the label:
    // the accessible name is the label followed by its explanation, and the
    // account-holder hint mentions a legal name too.
    const name = screen.getByLabelText(/^Legal name/) as HTMLInputElement;
    expect(name.value).toBe("");
    // …and it cannot be saved empty: the server requires a legal name, and the
    // form does not send a request it knows will be refused for being blank.
    const save = screen.getByRole("button", { name: strings.billingSave }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
  });

  test("sends only what changed, and clears a nullable field with null", async () => {
    reply("/billing/settings", "GET", { settings: STATED });
    ui("/billing/settings");

    await screen.findByDisplayValue("Alo Werkplaats B.V.");
    fireEvent.change(screen.getByLabelText(strings.billingFieldCity), {
      target: { value: "Rotterdam" },
    });
    // Clearing the bank account is how it comes off the record.
    fireEvent.change(screen.getByLabelText(strings.billingFieldIban, { exact: false }), {
      target: { value: "" },
    });

    reply("/billing/settings", "PATCH", { settings: { ...STATED, city: "Rotterdam", iban: null } });
    fireEvent.click(screen.getByRole("button", { name: strings.billingSave }));

    await waitFor(() => expect(lastWrite()?.method).toBe("PATCH"));
    // The two fields that moved, and nothing else — not the legal name, not
    // the VAT id, not the BIC.
    expect(lastWrite()?.body).toEqual({ city: "Rotterdam", iban: null });
  });

  test("a refusal is shown in the server's own words with the form intact", async () => {
    reply("/billing/settings", "GET", { settings: STATED });
    ui("/billing/settings");

    await screen.findByDisplayValue("Alo Werkplaats B.V.");
    fireEvent.change(screen.getByLabelText(strings.billingFieldIban, { exact: false }), {
      target: { value: "NL92ABNA0417164300" },
    });
    reply(
      "/billing/settings",
      "PATCH",
      { detail: "the check digits of this IBAN do not match; check for a typo" },
      422,
    );
    fireEvent.click(screen.getByRole("button", { name: strings.billingSave }));

    expect(
      await screen.findByText("the check digits of this IBAN do not match; check for a typo"),
    ).toBeTruthy();
    // What was typed is still there to be fixed in place.
    expect(screen.getByDisplayValue("NL92ABNA0417164300")).toBeTruthy();
  });

  test("the tab is in the module, and it is not what /billing lands on", async () => {
    ui("/billing");
    // Invoices stay the landing tab (B1.13/B1.14/B1.15); the details are a
    // thing you fill in once, not the reason you open billing. The approved
    // customer-first workflow lands on customers before documents.
    await waitFor(() =>
      expect(calls.some((c) => c.url.includes("/billing/customers"))).toBe(true),
    );
    expect(calls.some((c) => c.url.includes("/billing/settings"))).toBe(false);
    expect(screen.getByRole("link", { name: strings.billingSettings })).toBeTruthy();
  });
});
