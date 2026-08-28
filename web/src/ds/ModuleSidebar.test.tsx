// The module-sidebar contract: the column at desktop widths, the drawer at
// phone widths — and the drawer's keyboard behaviour, which is the reason the
// component exists at all. The two module copies it replaces had no Escape
// and no trap; these tests are what keeps the one implementation honest.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { ModuleSidebar } from "./ModuleSidebar";

/** matchMedia reporting a fixed answer — phone (true) or desktop (false). */
function installMatchMedia(matches: boolean) {
  vi.stubGlobal("matchMedia", (media: string) => ({
    matches,
    media,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function column() {
  return (
    <nav aria-label="places">
      <button type="button">First place</button>
      <button type="button">Second place</button>
    </nav>
  );
}

describe("at desktop widths", () => {
  test("renders the column untouched, with no drawer chrome", () => {
    installMatchMedia(false);
    render(
      <ModuleSidebar open={false} onClose={() => undefined} label="Places">
        {column()}
      </ModuleSidebar>,
    );
    expect(screen.getByRole("navigation", { name: "places" })).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("ignores open — the column never becomes a drawer", () => {
    installMatchMedia(false);
    render(
      <ModuleSidebar open onClose={() => undefined} label="Places">
        {column()}
      </ModuleSidebar>,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("at phone widths", () => {
  test("renders nothing while closed", () => {
    installMatchMedia(true);
    render(
      <ModuleSidebar open={false} onClose={() => undefined} label="Places">
        {column()}
      </ModuleSidebar>,
    );
    expect(screen.queryByRole("navigation")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("open renders a named dialog and focuses its first control", () => {
    installMatchMedia(true);
    render(
      <ModuleSidebar open onClose={() => undefined} label="Places">
        {column()}
      </ModuleSidebar>,
    );
    expect(screen.getByRole("dialog", { name: "Places" })).toBeTruthy();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "First place" }),
    );
  });

  test("Escape closes", () => {
    installMatchMedia(true);
    const onClose = vi.fn();
    render(
      <ModuleSidebar open onClose={onClose} label="Places">
        {column()}
      </ModuleSidebar>,
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("a backdrop tap closes", () => {
    installMatchMedia(true);
    const onClose = vi.fn();
    const { container } = render(
      <ModuleSidebar open onClose={onClose} label="Places">
        {column()}
      </ModuleSidebar>,
    );
    const backdrop = container.querySelector('[aria-hidden="true"]');
    expect(backdrop).not.toBeNull();
    fireEvent.click(backdrop!);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("Tab is trapped: last wraps to first, Shift+Tab from first to last", () => {
    installMatchMedia(true);
    render(
      <ModuleSidebar open onClose={() => undefined} label="Places">
        {column()}
      </ModuleSidebar>,
    );
    const first = screen.getByRole("button", { name: "First place" });
    const last = screen.getByRole("button", { name: "Second place" });
    last.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
  });

  test("closing gives focus back to the control that opened it", () => {
    installMatchMedia(true);
    render(<button type="button">toggle</button>);
    const toggle = screen.getByRole("button", { name: "toggle" });
    toggle.focus();
    const { unmount } = render(
      <ModuleSidebar open onClose={() => undefined} label="Places">
        {column()}
      </ModuleSidebar>,
    );
    expect(document.activeElement).not.toBe(toggle);
    unmount();
    expect(document.activeElement).toBe(toggle);
  });
});
