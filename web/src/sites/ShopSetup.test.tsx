// What the shop-setup screen must keep doing (S3.05b3): a proposal renders
// with every guess visibly flagged — stated prices prefilled, flagged blanks
// required, VAT always a guess with its basis beside it; approving applies
// ONLY through the owned routes (Billing's product door, the site's own
// shop-settings door) with exact bodies; a refused row shows the server's
// sentence and a retry re-sends only what is still pending; and an AI-less
// deployment keeps the manual path in view.
//
// Same harness as Tickets.test.tsx: the real API client and the real views
// run, and only the network is faked, so the URLs and bodies asserted here
// are the ones the wire-verified routes take.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import type { ShopConfigProposal } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

interface Reply {
  match: (url: string, method: string) => boolean;
  status: number;
  body: unknown;
}

const calls: Call[] = [];
let replies: Reply[] = [];

/** The site detail is the owner's unless a test seeds its own reply, so the
 *  screen exercises its full surface by default (S3.06a). */
function fallbackBody(url: string, method: string): unknown {
  if (method === "GET" && url.endsWith("/sites/site-1")) {
    return { id: "site-1", name: "Site one", canManageCollaborators: true };
  }
  return {};
}

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const index = replies.findIndex((reply) => reply.match(url, method));
  const answer =
    index === -1
      ? { status: 200, body: fallbackBody(url, method) }
      : (replies.splice(index, 1)[0] as Reply);
  return new Response(JSON.stringify(answer.body), {
    status: answer.status,
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

/** The ADR 0041 shapes: a stated price, a flagged blank, a stated rate. */
const PROPOSAL: ShopConfigProposal = {
  schema_version: 1,
  items: [
    {
      name: "Glaze Basics",
      kind: "stock",
      unit: "piece",
      price: { state: "stated", cents: 2_500 },
      vat_guess: { rate_bp: 600, basis: "Belgian reduced rate for printed books" },
      note: null,
    },
    {
      name: "Workshop seat",
      kind: "dated",
      unit: "seat",
      price: { state: "needs_input" },
      vat_guess: { rate_bp: 2_100, basis: "Belgian standard rate for workshops" },
      note: null,
    },
  ],
  shipping: { state: "stated", cents: 500 },
  shipping_note: null,
};

function productsReply(products: unknown[]): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/ticket-products"),
    status: 200,
    body: { currency: "EUR", currencyExponent: 2, products },
  };
}

function shippingReply(shippingCents: number): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/shop-settings"),
    status: 200,
    body: { shippingCents },
  };
}

function proposeReply(status: number, body: unknown): Reply {
  return {
    match: (url, method) =>
      method === "POST" && url.endsWith("/sites/shop-config/propose"),
    status,
    body,
  };
}

function productCreateReply(status: number, body: unknown): Reply {
  return {
    match: (url, method) => method === "POST" && url.endsWith("/billing/products"),
    status,
    body,
  };
}

function ui() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/shop-setup"]}>
      <Routes>
        <Route path="/sites/*" element={<SitesModule />} />
      </Routes>
    </MemoryRouter>,
  );
}

/** Types a description and proposes, with `PROPOSAL` as the answer. */
async function proposed() {
  fireEvent.change(await screen.findByLabelText(strings.sitesShopSetupDescribeLabel), {
    target: { value: "I sell pottery books and run workshops." },
  });
  replies.push(proposeReply(200, { proposal: PROPOSAL }));
  fireEvent.click(screen.getByRole("button", { name: strings.sitesShopSetupPropose }));
  await screen.findByText(strings.sitesShopSetupProposalTitle);
}

function productWrites(): Call[] {
  return calls.filter(
    (call) => call.method === "POST" && call.url.endsWith("/billing/products"),
  );
}

beforeEach(() => {
  calls.length = 0;
  replies = [productsReply([]), shippingReply(0)];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the shop-setup screen", () => {
  test("a proposal renders with every guess flagged", async () => {
    ui();
    await proposed();

    // The propose call carried the description and nothing else.
    const propose = calls.find((call) => call.url.endsWith("/shop-config/propose"));
    expect(propose?.body).toEqual({
      description: "I sell pottery books and run workshops.",
    });

    // The stated price arrives prefilled; the flagged blank stays blank and
    // says why it must be filled.
    const prices = screen.getAllByLabelText(strings.sitesShopSetupItemPrice("EUR"));
    expect((prices[0] as HTMLInputElement).value).toBe("25.00");
    expect((prices[1] as HTMLInputElement).value).toBe("");
    expect(screen.getAllByText(strings.sitesShopSetupPriceMissing).length).toBeGreaterThan(0);

    // VAT is a guess on every row — the badge and the basis sentence.
    expect(screen.getAllByText(strings.sitesShopSetupVatGuessBadge)).toHaveLength(2);
    expect(screen.getByText("Belgian reduced rate for printed books")).toBeTruthy();

    // The stated delivery rate is prefilled.
    const shipping = screen.getByLabelText(strings.sitesShopSetupShippingLabel("EUR"));
    expect((shipping as HTMLInputElement).value).toBe("5.00");

    // A flagged blank blocks approval, and the button says why.
    const approve = screen.getByRole("button", {
      name: strings.sitesShopSetupApprove(2),
    });
    expect(approve.hasAttribute("disabled")).toBe(true);
  });

  test("approving applies only through the owned routes, with exact bodies", async () => {
    ui();
    await proposed();

    const prices = screen.getAllByLabelText(strings.sitesShopSetupItemPrice("EUR"));
    fireEvent.change(prices[1] as HTMLInputElement, { target: { value: "60" } });

    replies.push(
      productCreateReply(200, { product: { id: "p1" } }),
      productCreateReply(200, { product: { id: "p2" } }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesShopSetupApprove(2) }),
    );
    await screen.findByText(strings.sitesShopSetupDone(2));

    // Two products through Billing's own door, each with the confirmed VAT
    // and the kind's stocked flag — never a number the screen computed with
    // floats.
    expect(productWrites().map((call) => call.body)).toEqual([
      {
        name: "Glaze Basics",
        unit: "piece",
        unitPriceCents: 2_500,
        vatRateBp: 600,
        stocked: true,
      },
      {
        name: "Workshop seat",
        unit: "seat",
        unitPriceCents: 6_000,
        vatRateBp: 2_100,
        stocked: false,
      },
    ]);

    // The delivery rate through the site's own settings door.
    const shippingWrite = calls.find(
      (call) => call.method === "PUT" && call.url.endsWith("/sites/site-1/shop-settings"),
    );
    expect(shippingWrite?.body).toEqual({ shippingCents: 500 });

    // A dated item was created, so the way to its events is offered.
    expect(screen.getByText(strings.sitesShopSetupNextTickets)).toBeTruthy();
  });

  test("a refused row shows the server's sentence, and retry re-sends only it", async () => {
    ui();
    await proposed();

    const prices = screen.getAllByLabelText(strings.sitesShopSetupItemPrice("EUR"));
    fireEvent.change(prices[1] as HTMLInputElement, { target: { value: "60" } });

    replies.push(
      productCreateReply(422, { detail: "a product with this name already exists" }),
      productCreateReply(200, { product: { id: "p2" } }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesShopSetupApprove(2) }),
    );

    // The server's own sentence, verbatim, on the row that failed — and the
    // action is now a retry.
    expect(
      await screen.findByText("a product with this name already exists"),
    ).toBeTruthy();
    const retry = await screen.findByRole("button", {
      name: strings.sitesShopSetupRetry,
    });
    expect(productWrites()).toHaveLength(2);

    replies.push(productCreateReply(200, { product: { id: "p1" } }));
    fireEvent.click(retry);
    await screen.findByText(strings.sitesShopSetupDone(2));

    // Exactly one more create: the refused row alone. The created row and
    // the saved delivery rate were not re-sent.
    expect(productWrites()).toHaveLength(3);
    const shippingWrites = calls.filter(
      (call) => call.method === "PUT" && call.url.endsWith("/shop-settings"),
    );
    expect(shippingWrites).toHaveLength(1);
  });

  test("a collaborator is told the screen is the owner's — as a status, with the price list never asked for", async () => {
    replies.push({
      match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
      status: 200,
      body: { id: "site-1", name: "Site one", canManageCollaborators: false },
    });
    ui();

    // The read-only fact is announced, not just printed (S3.06b): it lands
    // after the load, when a screen reader has already read the header.
    expect(await screen.findByText(strings.sitesCommerceReadOnly)).toBeTruthy();
    expect(
      screen
        .getAllByRole("status")
        .some((el) => el.textContent === strings.sitesCommerceReadOnly),
    ).toBe(true);
    // No describe box to type into, and the read the server would refuse a
    // collaborator was never made.
    expect(screen.queryByLabelText(strings.sitesShopSetupDescribeLabel)).toBeNull();
    expect(calls.some((call) => call.url.endsWith("/ticket-products"))).toBe(false);
    expect(calls.some((call) => call.url.endsWith("/shop-settings"))).toBe(false);
  });

  test("an AI-less deployment says so and keeps the manual path", async () => {
    ui();
    fireEvent.change(await screen.findByLabelText(strings.sitesShopSetupDescribeLabel), {
      target: { value: "I sell pottery books." },
    });
    replies.push(
      proposeReply(503, { reason: "unconfigured", detail: "no AI provider" }),
    );
    fireEvent.click(screen.getByRole("button", { name: strings.sitesShopSetupPropose }));

    expect(await screen.findByText(strings.sitesShopSetupUnconfigured)).toBeTruthy();
    // The by-hand doors stay in view.
    expect(screen.getByText(strings.sitesShopSetupManualTickets)).toBeTruthy();
    expect(screen.getByText(strings.sitesShopSetupManualCatalogs)).toBeTruthy();
  });
});
