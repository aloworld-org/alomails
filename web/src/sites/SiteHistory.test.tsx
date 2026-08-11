// The version-history screen (S2.04b): the visible list of versions, the
// preview of the one selected, and the one-click rollback with its undo.
//
// What is worth pinning here is the behaviour a person depends on: versions
// are named by date rather than by id, selecting one previews what it froze,
// restoring is one click and reports its result, and Undo puts back exactly
// the version that was live before — never the copy the restore just made.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { HistoryView } from "./HistoryView";

const mocks = vi.hoisted(() => ({
  site: vi.fn(),
  config: vi.fn(),
  publishes: vi.fn(),
  publishPages: vi.fn(),
  publishPreview: vi.fn(),
  comparePublishes: vi.fn(),
  restorePublish: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, useSitesApi: () => mocks };
});

const OLD = "publish-old";
const NEW = "publish-new";
const oldDate = "2026-08-09T09:15:00Z";
const newDate = "2026-08-10T16:40:00Z";

function version(id: string, publishedAt: string, current: boolean) {
  return {
    id,
    publishedAt,
    publishedBy: "user-1",
    defaultLocale: "en",
    enabledLocales: ["en"],
    restoredFrom: null,
    current,
    pages: 2,
    locales: ["en"],
    collections: 0,
  };
}

const formatted = (iso: string) =>
  new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(iso));

function renderHistory() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/history"]}>
      <Routes>
        <Route path="/sites/:siteId/history" element={<HistoryView />} />
        <Route path="/sites/:siteId" element={<p>site home</p>} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.site.mockResolvedValue({
    id: "site-1",
    name: "Alpha Bakery",
    subdomain: "alpha",
    status: "live",
    defaultLocale: "en",
    enabledLocales: ["en"],
    publish: { id: NEW, publishedAt: newDate },
    canManageCollaborators: true,
    theme: {},
  });
  mocks.config.mockResolvedValue({ domain: "alosites.com" });
  mocks.publishPages.mockResolvedValue([
    { pageId: "page-1", locale: "en", slug: "", title: "Home", home: true, navOrder: 0 },
    {
      pageId: "page-2",
      locale: "en",
      slug: "about",
      title: "About",
      home: false,
      navOrder: 1,
    },
  ]);
  mocks.publishPreview.mockResolvedValue("<html><body>frozen</body></html>");
  mocks.comparePublishes.mockResolvedValue({
    from: version(NEW, newDate, true),
    to: version(OLD, oldDate, false),
    identical: false,
    themeChanged: true,
    defaultLocaleChanged: false,
    localesAdded: [],
    localesRemoved: [],
    pages: [
      {
        pageId: "page-2",
        locale: "en",
        slug: "about",
        title: "About",
        change: "removed" as const,
        fields: [],
      },
    ],
    unchangedPages: 1,
    collections: [],
    unchangedCollections: 0,
  });
});

afterEach(cleanup);

describe("the version history screen", () => {
  test("previews an earlier version and puts it back online, undoably", async () => {
    mocks.publishes
      // First load: two versions, the newer one live.
      .mockResolvedValueOnce({
        publishes: [version(NEW, newDate, true), version(OLD, oldDate, false)],
        current: NEW,
      })
      // After the restore: a copy of the old version is live.
      .mockResolvedValueOnce({
        publishes: [
          { ...version("publish-copy", "2026-08-11T08:00:00Z", true), restoredFrom: OLD },
          version(NEW, newDate, false),
          version(OLD, oldDate, false),
        ],
        current: "publish-copy",
      })
      // After the undo: a copy of the newer version is live again.
      .mockResolvedValueOnce({
        publishes: [
          { ...version("publish-copy-2", "2026-08-11T08:05:00Z", true), restoredFrom: NEW },
          { ...version("publish-copy", "2026-08-11T08:00:00Z", false), restoredFrom: OLD },
          version(NEW, newDate, false),
          version(OLD, oldDate, false),
        ],
        current: "publish-copy-2",
      });
    mocks.restorePublish
      .mockResolvedValueOnce({ publishId: "publish-copy", restoredFrom: OLD })
      .mockResolvedValueOnce({ publishId: "publish-copy-2", restoredFrom: NEW });

    renderHistory();

    // Versions are listed by date, and the live one says so. (The live
    // version's date appears twice: in the list, and as the detail heading.)
    expect((await screen.findAllByText(formatted(newDate))).length).toBe(2);
    expect(screen.getByText(formatted(oldDate))).toBeTruthy();
    expect(screen.getByText(strings.sitesHistoryLiveNow)).toBeTruthy();
    // The live version is what the screen opens on, previewed from its home
    // page — no restore offered for what is already live.
    await waitFor(() =>
      expect(mocks.publishPreview).toHaveBeenCalledWith("site-1", NEW, "page-1", "en"),
    );
    expect(
      screen.queryByRole("button", { name: strings.sitesHistoryRestore }),
    ).toBeNull();

    // Selecting the earlier version previews it and says what would change.
    fireEvent.click(screen.getByText(formatted(oldDate)));
    await waitFor(() =>
      expect(mocks.publishPreview).toHaveBeenCalledWith("site-1", OLD, "page-1", "en"),
    );
    expect(mocks.comparePublishes).toHaveBeenCalledWith("site-1", NEW, OLD);
    expect(await screen.findByText(strings.sitesHistoryThemeChange)).toBeTruthy();
    expect(screen.getByText(strings.sitesHistoryPageGone("About"))).toBeTruthy();

    // One click puts it back online, and the result names the version.
    fireEvent.click(screen.getByRole("button", { name: strings.sitesHistoryRestore }));
    await waitFor(() =>
      expect(mocks.restorePublish).toHaveBeenCalledWith("site-1", OLD),
    );
    expect(
      await screen.findByText(strings.sitesHistoryRestored(formatted(oldDate))),
    ).toBeTruthy();
    expect(screen.getByRole("link", { name: "alpha.alosites.com" })).toBeTruthy();
    // The screen follows what the internet now shows — the copy the restore
    // made — so it offers no second restore of what is already live.
    await waitFor(() =>
      expect(mocks.publishPreview).toHaveBeenCalledWith(
        "site-1",
        "publish-copy",
        "page-1",
        "en",
      ),
    );
    expect(
      screen.queryByRole("button", { name: strings.sitesHistoryRestore }),
    ).toBeNull();

    // Undo puts back the version that was live before the restore.
    fireEvent.click(screen.getByRole("button", { name: strings.sitesHistoryUndo }));
    await waitFor(() =>
      expect(mocks.restorePublish).toHaveBeenLastCalledWith("site-1", NEW),
    );
    expect(
      await screen.findByText(strings.sitesHistoryUndone(formatted(newDate))),
    ).toBeTruthy();
    // The undo itself is not undoable in circles: the banner offers no second
    // Undo once the site is back where it started.
    expect(screen.queryByRole("button", { name: strings.sitesHistoryUndo })).toBeNull();
  });

  test("a site that never published is taught what history is, not shown a blank", async () => {
    mocks.publishes.mockResolvedValue({ publishes: [], current: null });

    renderHistory();

    expect(await screen.findByText(strings.sitesHistoryEmptyTitle)).toBeTruthy();
    expect(screen.getByText(strings.sitesHistoryEmptyBody)).toBeTruthy();
    expect(mocks.publishPages).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: strings.sitesBackToSite }));
    expect(await screen.findByText("site home")).toBeTruthy();
  });

  test("a failed history read is shown, verbatim, instead of an empty screen", async () => {
    const { SitesError } = await import("./api");
    mocks.publishes.mockRejectedValue(new SitesError(404, "no such site"));

    renderHistory();

    expect(await screen.findByText("no such site")).toBeTruthy();
  });
});
