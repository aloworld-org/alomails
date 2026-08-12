// What the catalog screen must keep doing (S2.12c): a site with nothing on
// offer explains what a catalog is rather than showing an empty table; a price
// is typed and sent verbatim for the server to read; and the server's own
// refusal sentence is what a person sees when a rule is broken.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { CatalogsView } from "./CatalogsView";
import { priceInput } from "./catalogPricing";
import type { SiteCatalog, SiteCatalogDetail, SiteCatalogItem } from "./types";

const mocks = vi.hoisted(() => ({
  catalogs: vi.fn(),
  catalog: vi.fn(),
  createCatalog: vi.fn(),
  updateCatalog: vi.fn(),
  deleteCatalog: vi.fn(),
  createCatalogCategory: vi.fn(),
  updateCatalogCategory: vi.fn(),
  deleteCatalogCategory: vi.fn(),
  createCatalogItem: vi.fn(),
  updateCatalogItem: vi.fn(),
  deleteCatalogItem: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, useSitesApi: () => mocks };
});

const CATALOG: SiteCatalog = {
  id: "catalog-1",
  name: "Saturday menu",
  currency: "EUR",
  currencyExponent: 2,
  ordersEnabled: false,
  createdAt: "2026-08-13T08:00:00Z",
  updatedAt: "2026-08-13T08:00:00Z",
};

const SOURDOUGH: SiteCatalogItem = {
  id: "item-1",
  categoryId: "group-1",
  name: "Sourdough loaf",
  slug: "sourdough-loaf",
  description: "Baked at six.",
  priceCents: 450,
  priceNote: "per loaf",
  imageBlobId: null,
  availability: "available",
  position: 0,
  sourceKey: null,
};

const DETAIL: SiteCatalogDetail = {
  catalog: CATALOG,
  categories: [{ id: "group-1", name: "Breads", slug: "breads", position: 0 }],
  items: [SOURDOUGH],
};

function view() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/catalogs"]}>
      <Routes>
        <Route path="/sites/:siteId/catalogs" element={<CatalogsView />} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
});

afterEach(cleanup);

describe("the catalog screen", () => {
  test("a site with nothing on offer is told what a catalog is", async () => {
    mocks.catalogs.mockResolvedValue([]);

    view();

    expect(await screen.findByText(strings.sitesCatalogNoneTitle)).toBeTruthy();
    expect(screen.getByText(strings.sitesCatalogNoneBody)).toBeTruthy();
    // The create form is already open on the first visit: the empty state
    // invites one action and that action is one click away.
    expect(
      screen.getByRole("button", { name: strings.sitesCatalogCreate }),
    ).toBeTruthy();
  });

  test("an empty catalog invites the first item instead of showing a table", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue({ ...DETAIL, items: [] });

    view();

    expect(await screen.findByText(strings.sitesCatalogNoItemsTitle)).toBeTruthy();
    expect(screen.getByText(strings.sitesCatalogNoItemsBody)).toBeTruthy();
  });

  test("shows a price in the catalog's currency and the note beside it", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue(DETAIL);

    view();

    expect(await screen.findByText("Sourdough loaf")).toBeTruthy();
    // The price, its note and the group the item is in, all in the row.
    expect(screen.getByText(/4[.,]50.*per loaf/)).toBeTruthy();
    expect(screen.getByText(/sourdough-loaf · Breads/)).toBeTruthy();
  });

  /** The panel header and the empty state both offer the same first action. */
  function openItemDialog() {
    const [add] = screen.getAllByRole("button", { name: strings.sitesCatalogAddItem });
    fireEvent.click(add as HTMLElement);
    return within(screen.getByRole("dialog"));
  }

  test("an item's price is sent exactly as it was typed", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue({ ...DETAIL, items: [] });
    mocks.createCatalogItem.mockResolvedValue(SOURDOUGH);

    view();
    await screen.findByText(strings.sitesCatalogNoItemsTitle);

    const dialog = openItemDialog();
    fireEvent.change(dialog.getByLabelText(strings.sitesCatalogItemName), {
      target: { value: "Sourdough loaf" },
    });
    fireEvent.change(
      dialog.getByLabelText(strings.sitesCatalogItemPrice(CATALOG.currency)),
      { target: { value: "4,50" } },
    );
    fireEvent.click(dialog.getByRole("button", { name: strings.sitesCatalogAddItem }));

    await waitFor(() => expect(mocks.createCatalogItem).toHaveBeenCalledTimes(1));
    expect(mocks.createCatalogItem).toHaveBeenCalledWith(
      "site-1",
      "catalog-1",
      expect.objectContaining({ name: "Sourdough loaf", price: "4,50", slug: "" }),
    );
  });

  test("the server's own refusal sentence is what a person sees", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue({ ...DETAIL, items: [] });
    const { SitesError } = await import("./api");
    mocks.createCatalogItem.mockRejectedValue(
      new SitesError(422, "that handle is already used in this catalog"),
    );

    view();
    await screen.findByText(strings.sitesCatalogNoItemsTitle);

    const dialog = openItemDialog();
    fireEvent.change(dialog.getByLabelText(strings.sitesCatalogItemName), {
      target: { value: "Sourdough loaf" },
    });
    fireEvent.click(dialog.getByRole("button", { name: strings.sitesCatalogAddItem }));

    expect(
      await screen.findByText("that handle is already used in this catalog"),
    ).toBeTruthy();
  });

  test("deleting an item asks once and only then deletes", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue(DETAIL);
    mocks.deleteCatalogItem.mockResolvedValue(undefined);

    view();

    const remove = await screen.findByRole("button", {
      name: strings.sitesCatalogItemDeleteLabel(SOURDOUGH.name),
    });
    fireEvent.click(remove);
    expect(mocks.deleteCatalogItem).not.toHaveBeenCalled();

    fireEvent.click(
      screen.getByRole("button", {
        name: strings.sitesCatalogItemDeleteConfirmLabel(SOURDOUGH.name),
      }),
    );
    await waitFor(() =>
      expect(mocks.deleteCatalogItem).toHaveBeenCalledWith("site-1", "catalog-1", "item-1"),
    );
  });

  test("turning ordering on is one switch on the catalog", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue(DETAIL);
    mocks.updateCatalog.mockResolvedValue({ ...CATALOG, ordersEnabled: true });

    view();

    fireEvent.click(await screen.findByText(strings.sitesCatalogOrders));
    fireEvent.click(screen.getByRole("button", { name: strings.sitesCatalogSave }));

    await waitFor(() =>
      expect(mocks.updateCatalog).toHaveBeenCalledWith("site-1", "catalog-1", {
        name: CATALOG.name,
        currency: "EUR",
        ordersEnabled: true,
      }),
    );
  });
});

describe("catalog prices", () => {
  test("minor units become an editable string and back", () => {
    expect(priceInput(450, 2)).toBe("4.50");
    expect(priceInput(4, 2)).toBe("0.04");
    expect(priceInput(1200, 0)).toBe("1200");
    expect(priceInput(null, 2)).toBe("");
  });
});
