// What the order inbox and the catalog section must keep doing (S2.12c2).
//
// The inbox: a site nobody has ordered from explains where orders come from
// rather than showing an empty table; an order's money is the server's, shown
// with the exponent the server sent and never recomputed; the workflow moves
// in both directions; and deleting an order — which carries a stranger's name
// and phone number — asks once before it happens.
//
// The section: it maps a page to a catalog and, optionally, one of its groups
// by HANDLE, and changing the catalog drops a handle that belonged to the old
// one rather than publishing a group that does not exist.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { OrdersView } from "./OrdersView";
import { SectionFormDialog } from "./SectionForm";
import type { SiteCatalog, SiteCatalogDetail, SiteDetail, SiteOrder } from "./types";

const mocks = vi.hoisted(() => ({
  site: vi.fn(),
  orders: vi.fn(),
  setOrderStatus: vi.fn(),
  deleteOrder: vi.fn(),
  ordersCsv: vi.fn(),
  catalogs: vi.fn(),
  catalog: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, useSitesApi: () => mocks };
});

const saved = vi.hoisted(() => ({ file: vi.fn() }));
vi.mock("../platform/download", () => ({ saveTextFile: saved.file }));

const SITE = { id: "site-1", name: "Bakery", subdomain: "bakery" } as unknown as SiteDetail;

const ORDER: SiteOrder = {
  id: "order-1",
  catalogId: "catalog-1",
  catalogName: "Saturday bake",
  currency: "EUR",
  currencyExponent: 2,
  customerName: "Ada Lovelace",
  customerEmail: "ada@example.test",
  customerPhone: "+32 2 555 01",
  note: "no nuts",
  totalCents: 1_350,
  status: "new",
  receivedAt: "2026-08-13T08:00:00Z",
  lines: [
    {
      itemSlug: "sourdough",
      itemName: "Sourdough",
      quantity: 3,
      unitPriceCents: 450,
      lineTotalCents: 1_350,
    },
    {
      itemSlug: "wedding-cake",
      itemName: "Wedding cake",
      quantity: 1,
      unitPriceCents: null,
      lineTotalCents: null,
    },
  ],
};

function inbox() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/orders"]}>
      <Routes>
        <Route path="/sites/:siteId/orders" element={<OrdersView />} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  saved.file.mockReset();
  mocks.site.mockResolvedValue(SITE);
});

afterEach(cleanup);

describe("the order inbox", () => {
  test("a site nobody has ordered from is told where orders come from", async () => {
    mocks.orders.mockResolvedValue([]);

    inbox();

    expect(await screen.findByText(strings.sitesNoOrdersTitle)).toBeTruthy();
    expect(screen.getByText(strings.sitesNoOrdersBody)).toBeTruthy();
  });

  test("an order shows its lines, its total and the unpriced line honestly", async () => {
    mocks.orders.mockResolvedValue([ORDER]);

    inbox();

    expect(await screen.findByText("Sourdough")).toBeTruthy();
    // 3 x 4.50 in the catalog's own currency, formatted from minor units with
    // the exponent the server sent.
    expect(screen.getAllByText(/13[.,]50/).length).toBeGreaterThan(0);
    // The cake was quoted by hand: no price at all, never shown as zero.
    expect(screen.getAllByText(strings.sitesOrderLineNoPrice).length).toBe(2);
    expect(screen.getByText(strings.sitesOrderQuotedHint)).toBeTruthy();
    expect(screen.getByText("no nuts")).toBeTruthy();
  });

  test("the workflow moves in both directions and shows the server's row", async () => {
    mocks.orders.mockResolvedValue([ORDER]);
    mocks.setOrderStatus.mockResolvedValue({ ...ORDER, status: "confirmed" });

    inbox();

    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesOrderStatusConfirmed }),
    );
    await waitFor(() =>
      expect(mocks.setOrderStatus).toHaveBeenCalledWith("site-1", "order-1", "confirmed"),
    );

    mocks.setOrderStatus.mockResolvedValue({ ...ORDER, status: "cancelled" });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesOrderStatusCancelled }));
    await waitFor(() =>
      expect(mocks.setOrderStatus).toHaveBeenLastCalledWith("site-1", "order-1", "cancelled"),
    );
    // And back again: nothing here is a one-way ratchet.
    mocks.setOrderStatus.mockResolvedValue({ ...ORDER, status: "confirmed" });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesOrderStatusConfirmed }));
    await waitFor(() => expect(mocks.setOrderStatus).toHaveBeenCalledTimes(3));
  });

  test("the server's own refusal sentence is what a person sees", async () => {
    mocks.orders.mockResolvedValue([ORDER]);
    const { SitesError } = await import("./api");
    mocks.setOrderStatus.mockRejectedValue(
      new SitesError(422, "posted is not an order status — it is one of new, confirmed, fulfilled or cancelled"),
    );

    inbox();

    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesOrderStatusFulfilled }),
    );

    expect(
      await screen.findByText(
        "posted is not an order status — it is one of new, confirmed, fulfilled or cancelled",
      ),
    ).toBeTruthy();
  });

  test("deleting an order asks once, and says what goes with it", async () => {
    mocks.orders.mockResolvedValue([ORDER]);
    mocks.deleteOrder.mockResolvedValue(undefined);

    inbox();

    fireEvent.click(await screen.findByRole("button", { name: strings.sitesOrderDelete }));
    expect(mocks.deleteOrder).not.toHaveBeenCalled();
    expect(screen.getByText(strings.sitesOrderDeleteHint)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: strings.sitesOrderDeleteConfirm }));
    await waitFor(() =>
      expect(mocks.deleteOrder).toHaveBeenCalledWith("site-1", "order-1"),
    );
    expect(await screen.findByText(strings.sitesNoOrdersTitle)).toBeTruthy();
  });

  test("the export is the server's CSV, saved under the site's own name", async () => {
    mocks.orders.mockResolvedValue([ORDER]);
    mocks.ordersCsv.mockResolvedValue("receivedAt,orderId\n");

    inbox();

    fireEvent.click(await screen.findByRole("button", { name: strings.sitesOrdersExport }));
    await waitFor(() => expect(mocks.ordersCsv).toHaveBeenCalledWith("site-1"));
    expect(saved.file).toHaveBeenCalledWith(
      "receivedAt,orderId\n",
      "orders-bakery.csv",
      "text/csv;charset=utf-8",
    );
  });

  test("filtering by state keeps the counts honest", async () => {
    mocks.orders.mockResolvedValue([ORDER, { ...ORDER, id: "order-2", status: "fulfilled" }]);

    inbox();

    const filters = await screen.findByRole("group", { name: strings.sitesOrderFilter });
    expect(
      within(filters).getByRole("button", {
        name: strings.sitesOrderFilterOption(strings.sitesOrderStatusNew, 1),
      }),
    ).toBeTruthy();
    fireEvent.click(
      within(filters).getByRole("button", {
        name: strings.sitesOrderFilterOption(strings.sitesOrderStatusCancelled, 0),
      }),
    );
    expect(screen.getByText(strings.sitesOrderFilterEmpty)).toBeTruthy();
  });
});

const CATALOG: SiteCatalog = {
  id: "catalog-1",
  name: "Saturday bake",
  currency: "EUR",
  currencyExponent: 2,
  ordersEnabled: true,
  createdAt: "2026-08-13T08:00:00Z",
  updatedAt: "2026-08-13T08:00:00Z",
};

const OTHER: SiteCatalog = { ...CATALOG, id: "catalog-2", name: "Courses", ordersEnabled: false };

const DETAIL: SiteCatalogDetail = {
  catalog: CATALOG,
  categories: [{ id: "group-1", name: "Breads", slug: "breads", position: 0 }],
  items: [],
};

function sectionForm(onSave: (section: unknown) => void) {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
      <Routes>
        <Route
          path="/sites/:siteId/pages/:pageId"
          element={
            <SectionFormDialog
              kind="catalog"
              busy={false}
              error={null}
              onClose={() => {}}
              onSave={onSave}
            />
          }
        />
      </Routes>
    </MemoryRouter>,
  );
}

describe("the catalog section", () => {
  test("a site with no catalog is told to make one instead of shown an empty menu", async () => {
    mocks.catalogs.mockResolvedValue([]);

    sectionForm(() => {});

    expect(await screen.findByText(strings.sitesCatalogSectionNoCatalogs)).toBeTruthy();
    expect(screen.getByText(strings.sitesCatalogSectionNoCatalogsHint)).toBeTruthy();
  });

  test("it saves the catalog and the group's handle, and says what ordering does", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue(DETAIL);
    const onSave = vi.fn();

    sectionForm(onSave);

    // The first catalog is chosen for the person rather than left blank.
    await waitFor(() => expect(mocks.catalog).toHaveBeenCalledWith("site-1", "catalog-1"));
    expect(await screen.findByText(strings.sitesCatalogSectionOrdersOn)).toBeTruthy();

    fireEvent.change(screen.getByLabelText(strings.sitesCatalogSectionGroup), {
      target: { value: "breads" },
    });
    fireEvent.change(screen.getByLabelText(strings.sitesCatalogSectionHeading), {
      target: { value: "Order for Saturday" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));

    expect(onSave).toHaveBeenCalledWith({
      type: "catalog",
      catalog_id: "catalog-1",
      heading: "Order for Saturday",
      category: "breads",
    });
  });

  test("choosing a different catalog drops a group handle that belonged to the old one", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG, OTHER]);
    mocks.catalog.mockResolvedValue(DETAIL);
    const onSave = vi.fn();

    sectionForm(onSave);

    await waitFor(() => expect(mocks.catalog).toHaveBeenCalledWith("site-1", "catalog-1"));
    fireEvent.change(screen.getByLabelText(strings.sitesCatalogSectionGroup), {
      target: { value: "breads" },
    });
    mocks.catalog.mockResolvedValue({ ...DETAIL, catalog: OTHER, categories: [] });
    fireEvent.change(screen.getByLabelText(strings.sitesCatalogSectionChoose), {
      target: { value: "catalog-2" },
    });

    expect(await screen.findByText(strings.sitesCatalogSectionOrdersOff)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));
    expect(onSave).toHaveBeenCalledWith({
      type: "catalog",
      catalog_id: "catalog-2",
      heading: undefined,
      category: undefined,
    });
  });
});
