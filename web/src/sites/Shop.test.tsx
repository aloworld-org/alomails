// What the shop window must keep doing (S3.05c): a site with nothing stocked
// is told the dependency rather than shown a picker with nothing in it; every
// price and shelf count on the screen is the owning seams' answer at this
// read, and a listing whose product left the price list says so instead of
// showing a stale price; the add dialog offers only what is not already
// listed and posts exactly the route's body; the delivery rate round-trips
// through the shop-settings route with the server's refusal spoken verbatim;
// and the page section saves the words alone — the shelf is never copied in.
//
// Same harness as Tickets.test.tsx: the real API client and the real views
// run, and only the network is faked, so the URLs and bodies asserted here
// are the ones the wire-verified routes take.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import { SectionFormDialog } from "./SectionForm";
import { formatPrice } from "./catalogPricing";
import type { SiteShopItemRow } from "./types";

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

const GUIDE = {
  id: "prod-1",
  name: "Field guide",
  unit: "piece",
  unitPriceCents: 2_400,
  vatRateBp: 2100,
  availableUnits: 7,
};

const PRESS = {
  id: "prod-2",
  name: "Letterpress print",
  unit: "piece",
  unitPriceCents: 4_500,
  vatRateBp: 2100,
  availableUnits: 1,
};

/** The guide on the shelf, resolved by the seams at this read. */
const LISTED: SiteShopItemRow = {
  id: "item-1",
  productId: "prod-1",
  productName: "Field guide",
  unit: "piece",
  unitPriceCents: 2_400,
  vatRateBp: 2100,
  availableUnits: 7,
  createdAt: "2026-08-16T08:00:00Z",
};

/** The same shape after its product was archived: the reference is stored,
 *  the price is nobody's any more, and the screen must say so. */
const ORPHANED: SiteShopItemRow = {
  ...LISTED,
  id: "item-2",
  productId: "prod-9",
  productName: null,
  unit: null,
  unitPriceCents: null,
  vatRateBp: null,
  availableUnits: null,
};

function productsReply(products: unknown[]): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/shop-products"),
    status: 200,
    body: { currency: "EUR", currencyExponent: 2, products },
  };
}

function itemsReply(items: SiteShopItemRow[]): Reply {
  return {
    match: (url, method) =>
      method === "GET" && url.endsWith("/sites/site-1/shop-items"),
    status: 200,
    body: { currency: "EUR", currencyExponent: 2, items },
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

function shippingReply(cents: number): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/shop-settings"),
    status: 200,
    body: { shippingCents: cents },
  };
}

function ui(path = "/sites/site-1/shop") {
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

describe("the shop screen", () => {
  test("nothing stocked is told as the dependency, not an empty picker", async () => {
    replies = [productsReply([]), itemsReply([]), shippingReply(0)];

    ui();

    expect(await screen.findByText(strings.sitesShopNoProducts)).toBeTruthy();
    expect(screen.getByText(strings.sitesShopNoProductsHint)).toBeTruthy();
    const add = screen.getByRole("button", { name: strings.sitesShopAddProduct });
    expect(add.hasAttribute("disabled")).toBe(true);
  });

  test("a collaborator reads the shelf and is offered nothing to change — and the price list is never asked for", async () => {
    replies = [collaboratorSiteReply(), itemsReply([LISTED]), shippingReply(450)];

    ui();

    expect(await screen.findByText(strings.sitesCommerceReadOnly)).toBeTruthy();
    // Announced, not just printed (S3.06b): the fact lands after the load,
    // when a screen reader has already moved past the header.
    expect(
      screen
        .getAllByRole("status")
        .some((el) => el.textContent === strings.sitesCommerceReadOnly),
    ).toBe(true);
    expect(screen.getByText("Field guide")).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: strings.sitesShopAddProduct }),
    ).toBeNull();
    expect(screen.queryByRole("button", { name: strings.sitesShopRemove })).toBeNull();
    expect(
      screen.queryByRole("button", { name: strings.sitesShopDeliveryChange }),
    ).toBeNull();
    expect(calls.some((call) => call.url.endsWith("/shop-products"))).toBe(false);
  });

  test("the shelf prices from the seams at this read; a gone product says so", async () => {
    replies = [productsReply([GUIDE]), itemsReply([LISTED, ORPHANED]), shippingReply(595)];

    ui();

    expect(await screen.findByText("Field guide")).toBeTruthy();
    expect(screen.getByText(formatPrice(2_400, "EUR", 2))).toBeTruthy();
    expect(screen.getByText(strings.sitesShopUnits(7))).toBeTruthy();
    // The archived product's row is a fact, not a stale price.
    expect(screen.getByText(strings.sitesShopGoneProduct)).toBeTruthy();
    expect(screen.getByText(strings.sitesShopNotStocked)).toBeTruthy();
    // The site's own delivery price, read from the settings route.
    expect(
      screen.getByText(
        strings.sitesShopDeliveryRate(formatPrice(595, "EUR", 2)),
        { exact: false },
      ),
    ).toBeTruthy();
  });

  test("adding offers only what is not listed and posts exactly the body", async () => {
    replies = [productsReply([GUIDE, PRESS]), itemsReply([LISTED]), shippingReply(0)];

    ui();

    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesShopAddProduct }),
    );

    // The guide is already on the shelf, so the picker offers the print only.
    const picker = screen.getByLabelText(strings.sitesShopProduct);
    const options = Array.from(picker.querySelectorAll("option"));
    expect(options.map((option) => option.value)).toEqual(["prod-2"]);
    expect(
      screen.getByText(
        strings.sitesShopProductOption("Letterpress print", formatPrice(4_500, "EUR", 2), 1),
      ),
    ).toBeTruthy();

    replies = [
      {
        match: (url, method) =>
          method === "POST" && url.endsWith("/sites/site-1/shop-items"),
        status: 200,
        body: { currency: "EUR", currencyExponent: 2, item: LISTED },
      },
      productsReply([GUIDE, PRESS]),
      itemsReply([LISTED]),
      shippingReply(0),
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesShopAddSubmit }));

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()).toMatchObject({
      method: "POST",
      body: { productId: "prod-2" },
    });
  });

  test("the store's refusal to add travels verbatim", async () => {
    replies = [productsReply([PRESS]), itemsReply([]), shippingReply(0)];

    ui();

    // Two doors to the same dialog exist here — the header button and the
    // empty shelf's invitation — so take the first rather than assume one.
    const [addDoor] = await screen.findAllByRole("button", {
      name: strings.sitesShopAddProduct,
    });
    fireEvent.click(addDoor!);

    const refusal = "that item is not a stocked product; the shop sells from the shelf";
    replies = [
      {
        match: (url, method) =>
          method === "POST" && url.endsWith("/sites/site-1/shop-items"),
        status: 422,
        body: { detail: refusal },
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesShopAddSubmit }));

    expect(await screen.findByText(refusal)).toBeTruthy();
  });

  test("remove asks once, then deletes the one listing", async () => {
    replies = [productsReply([GUIDE]), itemsReply([LISTED]), shippingReply(0)];

    ui();

    // First click arms; nothing is deleted yet.
    fireEvent.click(await screen.findByRole("button", { name: strings.sitesShopRemove }));
    expect(lastWrite()).toBeUndefined();
    expect(screen.getByText(strings.sitesShopRemoveHint)).toBeTruthy();
    // The arming renames a button, which nothing announces; the hint must be
    // a live region so the second step is said out loud (S3.06b).
    expect(
      screen
        .getAllByRole("status")
        .some((el) => el.textContent === strings.sitesShopRemoveHint),
    ).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: strings.sitesShopRemoveConfirm }));

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()).toMatchObject({ method: "DELETE" });
    expect(lastWrite()!.url.endsWith("/sites/site-1/shop-items/item-1")).toBe(true);
  });

  test("the delivery rate saves through the settings route, refusals verbatim", async () => {
    replies = [productsReply([GUIDE]), itemsReply([LISTED]), shippingReply(595)];

    ui();

    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesShopDeliveryChange }),
    );
    fireEvent.change(screen.getByLabelText(strings.sitesShopDeliveryLabel("EUR")), {
      target: { value: "1200,00" },
    });

    const refusal = "shipping must be between 0 and 100000 cents";
    replies = [
      {
        match: (url, method) => method === "PUT" && url.endsWith("/shop-settings"),
        status: 422,
        body: { detail: refusal },
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesShopDeliverySave }));

    expect(await screen.findByText(refusal)).toBeTruthy();
    expect(lastWrite()).toMatchObject({
      method: "PUT",
      body: { shippingCents: 120_000 },
    });
  });
});

describe("the shop page section", () => {
  test("the form saves the words alone, blanks absent — never the shelf", async () => {
    replies = [itemsReply([LISTED])];
    const saved: unknown[] = [];
    render(
      <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
        <Routes>
          <Route
            path="/sites/:siteId/pages/:pageId"
            element={
              <SectionFormDialog
                kind="shop"
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

    // The form says how many products the link will offer, read live.
    expect(await screen.findByText(strings.sitesShopSectionListed(1))).toBeTruthy();

    fireEvent.change(screen.getByLabelText(strings.sitesShopSectionHeading), {
      target: { value: "  From the press shop " },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));

    expect(saved).toEqual([{ type: "shop", heading: "From the press shop" }]);
  });

  test("a site with an empty shelf is told so in the form, with the way there", async () => {
    replies = [itemsReply([])];
    render(
      <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
        <Routes>
          <Route
            path="/sites/:siteId/pages/:pageId"
            element={
              <SectionFormDialog
                kind="shop"
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

    expect(await screen.findByText(strings.sitesShopSectionNoItems)).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.sitesShop })).toBeTruthy();
  });
});
