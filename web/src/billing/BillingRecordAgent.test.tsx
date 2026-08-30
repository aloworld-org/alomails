// What Billing's record agent promises (AW.7): a document says where it came
// from from its OWN record — an accepted offer, a recurring arrangement, the
// invoice it corrects — and says it does not know rather than inventing a
// source when the record carries none. The verbs offered follow the document's
// state, not its type, so a draft and a void keep their origin and their ask
// while being offered nothing they cannot do.
//
// Only the network is faked. The real panel, the real verb catalogue and the
// real directory client all run.
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { BillingRecordAgent, documentOrigin } from "./BillingRecordAgent";

let answers: { match: string; body: unknown }[] = [];
const calls: string[] = [];

const fakeFetch = vi.fn(async (url: string) => {
  calls.push(url);
  const hit = answers.find((a) => url.includes(a.match));
  return new Response(JSON.stringify(hit?.body ?? {}), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

const navigateSpy = vi.fn();
vi.mock("react-router-dom", () => ({
  useNavigate: () => navigateSpy,
}));

/** The Billing agent as the directory answers it: it offers four of the
 *  catalogue's six billing verbs, so only those four may become buttons. */
const DIRECTORY = {
  agents: [
    {
      id: "agent-billing",
      handle: "billing",
      name: "Billing",
      product: "billing",
      disabled: false,
      tools: [
        { name: "invoice_lookup", effect: "read" },
        { name: "draft_payment_reminder", effect: "write" },
        { name: "record_payment", effect: "write" },
        { name: "customer_lookup", effect: "read" },
      ],
    },
  ],
};

beforeEach(() => {
  calls.length = 0;
  answers = [{ match: "/chat/agents/directory", body: DIRECTORY }];
  navigateSpy.mockClear();
  fakeFetch.mockClear();
});

afterEach(cleanup);

test("a document's own record names its origin, most specific first", () => {
  expect(documentOrigin({ quoteId: "q-1" })).toEqual({
    kind: "quote",
    id: "q-1",
    label: null,
  });
  expect(documentOrigin({ scheduleId: "sch-1" })).toEqual({
    kind: "schedule",
    id: "sch-1",
    label: null,
  });
  // A credit note carrying the quote its own invoice grew from is still,
  // first and foremost, the correction of that invoice.
  expect(documentOrigin({ creditsInvoiceId: "inv-1", quoteId: "q-1" })).toEqual(
    { kind: "correction", id: "inv-1", label: null },
  );
  // A document a colleague typed carries none of the three, and says so by
  // answering `null` rather than inventing a source.
  expect(
    documentOrigin({ quoteId: null, scheduleId: null, creditsInvoiceId: null }),
  ).toBeNull();
});

test("an owed invoice cites its offer and offers the verbs that take it", async () => {
  render(
    <BillingRecordAgent
      recordKind="invoiceOwed"
      recordId="inv-2"
      recordLabel="INV-2026-00007"
      origin={{ kind: "quote", id: "q-9", label: null }}
    />,
  );

  expect(
    await screen.findByText(strings.recordAgentOriginQuoteUnnamed),
  ).toBeTruthy();
  // The offer has a screen, so the origin links back to it.
  expect(screen.getByText(strings.recordAgentOpenSource)).toBeTruthy();
  await waitFor(() => {
    expect(screen.getByText(strings.recordAgentVerbChaseInvoice)).toBeTruthy();
  });
  expect(screen.getByText(strings.recordAgentVerbRecordPayment)).toBeTruthy();
  // Offered by the catalogue for a customer, but this record is not one.
  expect(
    screen.queryByText(strings.recordAgentVerbCustomerStanding),
  ).toBeNull();
});

test("a schedule's draft says so, and is offered nothing it cannot do", async () => {
  render(
    <BillingRecordAgent
      recordKind="invoice"
      recordId="inv-3"
      recordLabel={strings.billingDraftInvoice}
      origin={{ kind: "schedule", id: "sch-4", label: null }}
    />,
  );

  expect(
    await screen.findByText(strings.recordAgentOriginSchedule),
  ).toBeTruthy();
  // A recurring arrangement has no record screen, so there is nothing to open.
  expect(screen.queryByText(strings.recordAgentOpenSource)).toBeNull();
  // A draft is owed by nobody: it keeps its origin and its ask, and no verb.
  await waitFor(() => {
    expect(
      screen.getByLabelText(strings.recordAgentAskPlaceholder("billing")),
    ).toBeTruthy();
  });
  expect(screen.queryByText(strings.recordAgentVerbChaseInvoice)).toBeNull();
  expect(screen.queryByText(strings.recordAgentVerbRecordPayment)).toBeNull();
});

test("a credit note names the invoice it corrects, and links to it", async () => {
  render(
    <BillingRecordAgent
      recordKind="invoice"
      recordId="inv-9"
      recordLabel="CN-2026-00002"
      origin={documentOrigin({ creditsInvoiceId: "inv-2" })}
    />,
  );

  expect(
    await screen.findByText(strings.recordAgentOriginCorrection),
  ).toBeTruthy();
  screen.getByText(strings.recordAgentOpenSource).click();
  await waitFor(() => {
    expect(navigateSpy).toHaveBeenCalledWith("/billing/invoices/inv-2");
  });
});

test("a customer with no provenance says so, and still offers its verbs", async () => {
  render(
    <BillingRecordAgent
      recordKind="customer"
      recordId="c-1"
      recordLabel="Acme GmbH"
    />,
  );

  expect(await screen.findByText(strings.recordAgentOriginNone)).toBeTruthy();
  expect(screen.queryByText(strings.recordAgentOpenSource)).toBeNull();
  await waitFor(() => {
    expect(
      screen.getByText(strings.recordAgentVerbCustomerStanding),
    ).toBeTruthy();
  });
  // The directory did not offer these two, so the panel does not either — a
  // verb the boundary would refuse is never a button.
  expect(screen.queryByText(strings.recordAgentVerbCustomerUnpaid)).toBeNull();
  expect(
    screen.queryByText(strings.recordAgentVerbCustomerOpenQuotes),
  ).toBeNull();
  // Nothing was read but the directory: the panel is quiet until asked.
  expect(calls).toHaveLength(1);
});
