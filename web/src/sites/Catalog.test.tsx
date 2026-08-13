// What the catalog screen must keep doing (S2.12c): a site with nothing on
// offer explains what a catalog is rather than showing an empty table; a price
// is typed and sent verbatim for the server to read; the server's own refusal
// sentence is what a person sees when a rule is broken; and an item's
// photograph (S2.12c3) is chosen, described, kept across an edit and removed
// with the words that described it.
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
  siteImage: vi.fn(),
}));

/** Uploads go through Drive at the jmap-client seam; nothing else is faked. */
const driveUploadBlob = vi.hoisted(() => vi.fn());

vi.mock("../jmap", () => ({
  useJmapClient: () => ({ driveUploadBlob }),
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
  imageAlt: null,
  availability: "available",
  position: 0,
  sourceKey: null,
};

/** The same loaf, photographed and described. */
const PHOTOGRAPHED: SiteCatalogItem = {
  ...SOURDOUGH,
  imageBlobId: "blob-loaf",
  imageAlt: "A dark round loaf on the peel",
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
  driveUploadBlob.mockReset();
  mocks.siteImage.mockResolvedValue(new Blob(["loaf"], { type: "image/png" }));
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

  /** The hidden file input the photo buttons drive; there is exactly one, and
   *  it has no accessible name of its own because the button is the control. */
  function photoPicker(): HTMLInputElement {
    const picker = screen.getByRole("dialog").querySelector("input[type=file]");
    if (picker === null) throw new Error("the dialog offers no file picker");
    return picker as HTMLInputElement;
  }

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

  test("an item without a photo says so, and what happens without one", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue({ ...DETAIL, items: [] });

    view();
    await screen.findByText(strings.sitesCatalogNoItemsTitle);

    const dialog = openItemDialog();
    expect(dialog.getByText(strings.sitesCatalogItemPhotoNone)).toBeTruthy();
    expect(dialog.getByText(strings.sitesCatalogItemPhotoNoneHint)).toBeTruthy();
    // Nothing to describe yet, so nothing asks for a description.
    expect(dialog.queryByLabelText(strings.sitesCatalogItemPhotoAlt)).toBeNull();
  });

  test("a photo is uploaded through Drive and sent with the words about it", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue({ ...DETAIL, items: [] });
    mocks.createCatalogItem.mockResolvedValue(PHOTOGRAPHED);
    driveUploadBlob.mockResolvedValue({ blobId: "blob-loaf" });

    view();
    await screen.findByText(strings.sitesCatalogNoItemsTitle);

    const dialog = openItemDialog();
    fireEvent.change(dialog.getByLabelText(strings.sitesCatalogItemName), {
      target: { value: "Sourdough loaf" },
    });
    const file = new File(["bytes"], "loaf.png", { type: "image/png" });
    fireEvent.change(photoPicker(), { target: { files: [file] } });

    // Until it is described, the form says the card will fall back to the name.
    const alt = await dialog.findByLabelText(strings.sitesCatalogItemPhotoAlt);
    expect(dialog.getByText(strings.sitesCatalogItemPhotoAltMissing)).toBeTruthy();
    fireEvent.change(alt, { target: { value: "A dark round loaf on the peel" } });

    fireEvent.click(dialog.getByRole("button", { name: strings.sitesCatalogAddItem }));
    await waitFor(() => expect(mocks.createCatalogItem).toHaveBeenCalledTimes(1));
    expect(mocks.createCatalogItem).toHaveBeenCalledWith(
      "site-1",
      "catalog-1",
      expect.objectContaining({
        imageBlobId: "blob-loaf",
        imageAlt: "A dark round loaf on the peel",
      }),
    );
  });

  test("editing an item keeps the photo it already had", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue({ ...DETAIL, items: [PHOTOGRAPHED] });
    mocks.updateCatalogItem.mockResolvedValue(PHOTOGRAPHED);

    view();

    fireEvent.click(
      await screen.findByRole("button", {
        name: strings.sitesCatalogEditItem(PHOTOGRAPHED.name),
      }),
    );
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByLabelText(strings.sitesCatalogItemName), {
      target: { value: "Sourdough loaf, large" },
    });
    fireEvent.click(dialog.getByRole("button", { name: strings.sitesCatalogSaveItem }));

    await waitFor(() => expect(mocks.updateCatalogItem).toHaveBeenCalledTimes(1));
    // A whole replace that forgot the picture would publish a card without one.
    expect(mocks.updateCatalogItem).toHaveBeenCalledWith(
      "site-1",
      "catalog-1",
      "item-1",
      expect.objectContaining({
        name: "Sourdough loaf, large",
        imageBlobId: "blob-loaf",
        imageAlt: "A dark round loaf on the peel",
      }),
    );
  });

  test("removing the photo takes the words about it with it", async () => {
    mocks.catalogs.mockResolvedValue([CATALOG]);
    mocks.catalog.mockResolvedValue({ ...DETAIL, items: [PHOTOGRAPHED] });
    mocks.updateCatalogItem.mockResolvedValue({ ...SOURDOUGH });

    view();

    fireEvent.click(
      await screen.findByRole("button", {
        name: strings.sitesCatalogEditItem(PHOTOGRAPHED.name),
      }),
    );
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.click(
      dialog.getByRole("button", { name: strings.sitesCatalogItemPhotoRemove }),
    );
    expect(dialog.getByText(strings.sitesCatalogItemPhotoNone)).toBeTruthy();
    fireEvent.click(dialog.getByRole("button", { name: strings.sitesCatalogSaveItem }));

    await waitFor(() => expect(mocks.updateCatalogItem).toHaveBeenCalledTimes(1));
    expect(mocks.updateCatalogItem).toHaveBeenCalledWith(
      "site-1",
      "catalog-1",
      "item-1",
      expect.objectContaining({ imageBlobId: null, imageAlt: "" }),
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
