// A page behind a password (S2.06b): the visible control.
//
// What is worth pinning is what the owner depends on. The screen states who
// can read the page rather than naming a setting; the password it sends is
// the one that was typed and is never rendered back; a refusal appears in the
// server's own words; and taking the password off — the one gesture that
// exposes the page to the internet and cannot be undone — asks twice before
// it does it.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { PagePassword } from "./PagePassword";
import { SitesError } from "./api";
import type { SitePageProtection } from "./types";

const mocks = vi.hoisted(() => ({
  pagePassword: vi.fn(),
  setPagePassword: vi.fn(),
  removePagePassword: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, useSitesApi: () => mocks };
});

const SET_ON = "2026-08-12T09:30:00Z";

const PUBLIC_PAGE: SitePageProtection = {
  protected: false,
  pageId: null,
  createdAt: null,
  updatedAt: null,
};

const PROTECTED_PAGE: SitePageProtection = {
  protected: true,
  pageId: "page-1",
  createdAt: SET_ON,
  updatedAt: SET_ON,
};

const readable = (iso: string) =>
  new Intl.DateTimeFormat(undefined, { dateStyle: "long" }).format(
    new Date(iso),
  );

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.pagePassword.mockResolvedValue(PUBLIC_PAGE);
});

afterEach(cleanup);

test("the compact editor row keeps its access action on protected spacing", async () => {
  render(<PagePassword siteId="site-1" pageId="page-1" compact />);

  const action = await screen.findByRole("button", {
    name: strings.sitesPagePasswordProtect,
  });
  expect(action).toBeTruthy();
  expect(action.parentElement?.className).toContain("w-full");
});

test("the navigation control keeps access loaded and reveals its focused dialog", async () => {
  const seen: boolean[] = [];
  render(
    <PagePassword
      siteId="site-1"
      pageId="page-1"
      navigation
      onChange={(isProtected) => seen.push(isProtected)}
    />,
  );

  const access = screen.getByRole("button", { name: strings.sitesPageAccess });
  expect(screen.queryByRole("dialog")).toBeNull();
  await waitFor(() => expect(seen).toEqual([false]));

  fireEvent.click(access);
  expect(
    screen.getByRole("dialog", { name: strings.sitesPageAccess }),
  ).toBeTruthy();
  expect(await screen.findByText(strings.sitesPagePasswordPublic)).toBeTruthy();
});

test("a public page says the internet can read it, and offers the password", async () => {
  const seen: boolean[] = [];
  render(
    <PagePassword
      siteId="site-1"
      pageId="page-1"
      onChange={(isProtected) => seen.push(isProtected)}
    />,
  );

  expect(await screen.findByText(strings.sitesPagePasswordPublic)).toBeTruthy();
  expect(
    screen.getByRole("button", { name: strings.sitesPagePasswordProtect }),
  ).toBeTruthy();
  // Nothing to remove while there is nothing to remove.
  expect(
    screen.queryByRole("button", { name: strings.sitesPagePasswordRemove }),
  ).toBeNull();
  await waitFor(() => expect(seen).toEqual([false]));
});

test("protecting sends what was typed and states what visitors now meet", async () => {
  mocks.setPagePassword.mockResolvedValue(PROTECTED_PAGE);
  const seen: boolean[] = [];
  render(
    <PagePassword
      siteId="site-1"
      pageId="page-1"
      onChange={(isProtected) => seen.push(isProtected)}
    />,
  );

  fireEvent.click(
    await screen.findByRole("button", {
      name: strings.sitesPagePasswordProtect,
    }),
  );

  const field = screen.getByLabelText(strings.sitesPagePasswordField);
  // The field starts empty and hides what is typed: no stored password is
  // ever rendered, because no read on the server answers one.
  expect((field as HTMLInputElement).value).toBe("");
  expect((field as HTMLInputElement).type).toBe("password");
  fireEvent.change(field, { target: { value: "roastery-2026" } });

  // The show toggle is what stands in for a confirm field.
  fireEvent.click(
    screen.getByRole("button", { name: strings.sitesPagePasswordShow }),
  );
  expect(
    (screen.getByLabelText(strings.sitesPagePasswordField) as HTMLInputElement)
      .type,
  ).toBe("text");

  fireEvent.click(
    screen.getByRole("button", { name: strings.sitesPagePasswordProtect }),
  );

  await waitFor(() =>
    expect(mocks.setPagePassword).toHaveBeenCalledWith(
      "site-1",
      "page-1",
      "roastery-2026",
    ),
  );
  expect(
    await screen.findByText(
      strings.sitesPagePasswordProtected(readable(SET_ON)),
    ),
  ).toBeTruthy();
  expect(screen.getByText(strings.sitesPagePasswordProtectedHint)).toBeTruthy();
  expect(screen.getByText(strings.sitesPagePasswordSaved)).toBeTruthy();
  await waitFor(() => expect(seen).toEqual([false, true]));
});

test("an empty field is refused here, without asking the server", async () => {
  render(<PagePassword siteId="site-1" pageId="page-1" />);

  fireEvent.click(
    await screen.findByRole("button", {
      name: strings.sitesPagePasswordProtect,
    }),
  );
  fireEvent.click(
    screen.getByRole("button", { name: strings.sitesPagePasswordProtect }),
  );

  expect(
    await screen.findByText(strings.sitesPagePasswordMissing),
  ).toBeTruthy();
  expect(mocks.setPagePassword).not.toHaveBeenCalled();
});

test("a password the server refuses is reported in the server's own words", async () => {
  mocks.setPagePassword.mockRejectedValue(
    new SitesError(422, "a page password must be at least 8 characters"),
  );
  render(<PagePassword siteId="site-1" pageId="page-1" />);

  fireEvent.click(
    await screen.findByRole("button", {
      name: strings.sitesPagePasswordProtect,
    }),
  );
  fireEvent.change(screen.getByLabelText(strings.sitesPagePasswordField), {
    target: { value: "short" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: strings.sitesPagePasswordProtect }),
  );

  expect(
    await screen.findByText("a page password must be at least 8 characters"),
  ).toBeTruthy();
  // The refusal keeps the form open with what was typed, so it can be fixed.
  expect(
    (screen.getByLabelText(strings.sitesPagePasswordField) as HTMLInputElement)
      .value,
  ).toBe("short");
});

test("removing the password asks a second time, then says the page is public", async () => {
  mocks.pagePassword.mockResolvedValue(PROTECTED_PAGE);
  mocks.removePagePassword.mockResolvedValue(PUBLIC_PAGE);
  const seen: boolean[] = [];
  render(
    <PagePassword
      siteId="site-1"
      pageId="page-1"
      onChange={(isProtected) => seen.push(isProtected)}
    />,
  );

  fireEvent.click(
    await screen.findByRole("button", {
      name: strings.sitesPagePasswordRemove,
    }),
  );
  // The first click only arms it: nothing is public yet.
  expect(mocks.removePagePassword).not.toHaveBeenCalled();

  fireEvent.click(
    screen.getByRole("button", {
      name: strings.sitesPagePasswordRemoveConfirm,
    }),
  );

  await waitFor(() =>
    expect(mocks.removePagePassword).toHaveBeenCalledWith("site-1", "page-1"),
  );
  expect(
    await screen.findByText(strings.sitesPagePasswordRemoved),
  ).toBeTruthy();
  expect(screen.getByText(strings.sitesPagePasswordPublic)).toBeTruthy();
  await waitFor(() => expect(seen).toEqual([true, false]));
});

test("a protected page in a multilingual site says the password holds for every language", async () => {
  mocks.pagePassword.mockResolvedValue(PROTECTED_PAGE);
  render(<PagePassword siteId="site-1" pageId="page-1" multilingual />);

  expect(
    await screen.findByText(strings.sitesPagePasswordEveryLanguage),
  ).toBeTruthy();
});

test("a protection the server could not read leaves the panel honest", async () => {
  mocks.pagePassword.mockRejectedValue(new SitesError(0, null));
  render(<PagePassword siteId="site-1" pageId="page-1" />);

  expect(
    await screen.findByText(strings.sitesPagePasswordLoadFailed),
  ).toBeTruthy();
  // It says it does not know rather than guessing "anyone can read this" —
  // the reassuring guess is the dangerous one.
  expect(screen.getByText(strings.sitesPagePasswordUnknown)).toBeTruthy();
  expect(screen.queryByText(strings.sitesPagePasswordPublic)).toBeNull();
  expect(
    screen.queryByRole("button", { name: strings.sitesPagePasswordProtect }),
  ).toBeNull();
});
