// What the box office must keep doing (S3.04f3): a site whose price list is
// empty is told the dependency rather than shown a picker with nothing in it;
// every price on the screen is the list's answer at this read, and an event
// whose item left the list says so instead of showing a stale price; a new
// event posts exactly the route's body; the server's own refusal sentence is
// what a person reads; and the page section saves the words alone — the
// events are never copied into it.
//
// Same harness as Booking.test.tsx: the real API client and the real views
// run, and only the network is faked, so the URLs and bodies asserted here
// are the ones the wire-verified routes take.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import { SectionFormDialog } from "./SectionForm";
import type { SiteTicketEvent } from "./types";

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
 *  screens exercise their full surface by default (S3.06a). */
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

const WORKSHOP = {
  id: "prod-1",
  name: "Letterpress workshop",
  unit: "seat",
  unitPriceCents: 8_500,
  vatRateBp: 2100,
};

/** One event mid-sale: two seats sold, one in a checkout. */
const EVENING: SiteTicketEvent = {
  id: "evt-1",
  productId: "prod-1",
  productName: "Letterpress workshop",
  unitPriceCents: 8_500,
  vatRateBp: 2100,
  startsAt: "2026-09-16T17:00:00Z",
  capacity: 12,
  sold: 2,
  held: 1,
  remaining: 9,
  createdAt: "2026-08-16T08:00:00Z",
  updatedAt: "2026-08-16T08:00:00Z",
};

/** The same shape after its item was archived: the reference is stored, the
 *  price is nobody's any more, and the screen must say so. */
const ORPHANED: SiteTicketEvent = {
  ...EVENING,
  id: "evt-2",
  productName: null,
  unitPriceCents: null,
  vatRateBp: null,
  sold: 0,
  held: 0,
  remaining: 12,
};

function productsReply(products: unknown[]): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/ticket-products"),
    status: 200,
    body: { currency: "EUR", currencyExponent: 2, products },
  };
}

/** A restricted collaborator's view of the site (S3.06a): the server refuses
 *  them the picker and the write verbs, so the screen must not offer either. */
function collaboratorSiteReply(): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/sites/site-1"),
    status: 200,
    body: { id: "site-1", name: "Site one", canManageCollaborators: false },
  };
}

function eventsReply(events: SiteTicketEvent[]): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/tickets"),
    status: 200,
    body: { currency: "EUR", currencyExponent: 2, events },
  };
}

function ui(path = "/sites/site-1/tickets") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/sites/*" element={<SitesModule />} />
      </Routes>
    </MemoryRouter>,
  );
}

function lastWrite(): Call | undefined {
  return calls.filter((call) => call.method !== "GET").at(-1);
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the tickets screen", () => {
  test("an empty price list is told as the dependency, not an empty picker", async () => {
    replies = [productsReply([]), eventsReply([])];

    ui();

    expect(await screen.findByText(strings.sitesTicketNoProducts)).toBeTruthy();
    expect(screen.getByText(strings.sitesTicketNoProductsHint)).toBeTruthy();
    // Nothing can be created until Billing has an item, so the button says no.
    const create = screen.getByRole("button", { name: strings.sitesNewTicketEvent });
    expect(create.hasAttribute("disabled")).toBe(true);
  });

  test("a collaborator reads the box office and is offered nothing to change — and the price list is never asked for", async () => {
    replies = [collaboratorSiteReply(), eventsReply([EVENING])];

    ui();

    expect(await screen.findByText(strings.sitesCommerceReadOnly)).toBeTruthy();
    // Announced, not just printed (S3.06b): the fact lands after the load,
    // when a screen reader has already moved past the header.
    expect(
      screen
        .getAllByRole("status")
        .some((el) => el.textContent === strings.sitesCommerceReadOnly),
    ).toBe(true);
    expect(screen.getByText("Letterpress workshop")).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: strings.sitesNewTicketEvent }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: strings.sitesTicketChangeCapacity }),
    ).toBeNull();
    expect(screen.queryByRole("button", { name: strings.sitesTicketDelete })).toBeNull();
    expect(calls.some((call) => call.url.endsWith("/ticket-products"))).toBe(false);
  });

  test("the list prices from the seam at this read; a gone item says so", async () => {
    replies = [productsReply([WORKSHOP]), eventsReply([EVENING, ORPHANED])];

    ui();

    expect(await screen.findByText("Letterpress workshop")).toBeTruthy();
    // The live seat arithmetic, in words: sold, left of capacity, in checkout.
    expect(screen.getByText(strings.sitesTicketSeatsCell(2, 9, 12))).toBeTruthy();
    expect(screen.getByText(strings.sitesTicketHeld(1))).toBeTruthy();
    // The archived item's row is a fact, not a stale price.
    expect(screen.getByText(strings.sitesTicketGoneProduct)).toBeTruthy();
  });

  test("a new event posts exactly the route's body", async () => {
    replies = [productsReply([WORKSHOP]), eventsReply([])];

    ui();

    fireEvent.click(await screen.findByText(strings.sitesNoTicketEventsTitle));
    fireEvent.click(
      screen.getAllByRole("button", { name: strings.sitesNewTicketEvent }).at(-1)!,
    );

    const starts = screen.getByLabelText(strings.sitesTicketEventStartsAt);
    fireEvent.change(starts, { target: { value: "2026-09-16T19:00" } });
    fireEvent.change(screen.getByLabelText(strings.sitesTicketEventCapacity), {
      target: { value: "12" },
    });

    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-1/tickets"),
        status: 200,
        body: { currency: "EUR", currencyExponent: 2, event: EVENING },
      },
      productsReply([WORKSHOP]),
      eventsReply([EVENING]),
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesTicketCreateSubmit }));

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()).toMatchObject({
      method: "POST",
      body: {
        productId: "prod-1",
        // The instant the local picker named, spoken as RFC 3339 UTC — the
        // same conversion the view does, computed here in the same zone.
        startsAt: new Date("2026-09-16T19:00").toISOString(),
        capacity: 12,
      },
    });
  });

  test("shrinking below the seats already taken shows the server's sentence", async () => {
    replies = [productsReply([WORKSHOP]), eventsReply([EVENING])];

    ui();

    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesTicketChangeCapacity }),
    );
    fireEvent.change(screen.getByLabelText(strings.sitesTicketEventCapacity), {
      target: { value: "2" },
    });

    const refusal = "3 seats are already sold or on hold; capacity cannot go below that";
    replies = [
      {
        match: (url, method) =>
          method === "PUT" && url.endsWith("/sites/site-1/tickets/evt-1"),
        status: 422,
        body: { detail: refusal },
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesTicketCapacitySubmit }));

    expect(await screen.findByText(refusal)).toBeTruthy();
  });

  test("delete asks once, then deletes; a sold event's refusal is spoken", async () => {
    replies = [productsReply([WORKSHOP]), eventsReply([EVENING])];

    ui();

    // First click arms; nothing is deleted yet.
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesTicketDelete }));
    expect(lastWrite()).toBeUndefined();

    const refusal = "tickets have been sold to this event; it can no longer be deleted";
    replies = [
      {
        match: (url, method) =>
          method === "DELETE" && url.endsWith("/sites/site-1/tickets/evt-1"),
        status: 422,
        body: { detail: refusal },
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesTicketDeleteConfirm }));

    expect(await screen.findByText(refusal)).toBeTruthy();
  });
});

describe("the tickets page section", () => {
  test("the form saves the words alone, blanks absent — never the events", async () => {
    replies = [eventsReply([EVENING])];
    const saved: unknown[] = [];
    render(
      <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
        <Routes>
          <Route
            path="/sites/:siteId/pages/:pageId"
            element={
              <SectionFormDialog
                kind="tickets"
                busy={false}
                error={null}
                onClose={() => undefined}
                onSave={(section) => saved.push(section)}
              />
            }
          />
        </Routes>
      </MemoryRouter>,
    );

    // The form says how many events the link will offer, read live.
    expect(await screen.findByText(strings.sitesTicketSectionOnSale(1))).toBeTruthy();

    fireEvent.change(screen.getByLabelText(strings.sitesTicketSectionHeading), {
      target: { value: "  Evenings at the press " },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));

    expect(saved).toEqual([{ type: "tickets", heading: "Evenings at the press" }]);
  });

  test("a site with nothing on sale is told so in the form, with the way there", async () => {
    replies = [eventsReply([])];
    render(
      <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
        <Routes>
          <Route
            path="/sites/:siteId/pages/:pageId"
            element={
              <SectionFormDialog
                kind="tickets"
                busy={false}
                error={null}
                onClose={() => undefined}
                onSave={() => undefined}
              />
            }
          />
        </Routes>
      </MemoryRouter>,
    );

    expect(await screen.findByText(strings.sitesTicketSectionNoEvents)).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.sitesTickets })).toBeTruthy();
  });
});
