// What eleven hand-rolled toolbars did not do.
//
// The appearance was never in dispute — four of the eleven were byte-identical.
// These are the properties that were missing from all of them, and every one is
// invisible until the screen is used with something other than a mouse.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { Toolbar, ToolbarGroup, ToolbarSpacer } from "./Toolbar";

afterEach(cleanup);

function formatting() {
  return render(
    <Toolbar label="Formatting" keyboard="roving" density="compact">
      <ToolbarGroup label="Text style">
        <button>Bold</button>
        <button>Italic</button>
      </ToolbarGroup>
      <ToolbarSpacer />
      <button>Link</button>
    </Toolbar>,
  );
}

function stops() {
  return screen.getAllByRole("button").map((el) => el.tabIndex);
}

describe("a toolbar is a group, and says which one", () => {
  test("it names itself", () => {
    render(
      <Toolbar label="Invoice list">
        <button>New</button>
      </Toolbar>,
    );
    // All eleven were bare `<div>`s, so a screen with two of them — tasks has
    // exactly that — presented two indistinguishable runs of buttons.
    expect(screen.getByRole("group", { name: "Invoice list" })).toBeDefined();
  });

  test("a mixed toolbar is not announced as a toolbar", () => {
    render(
      <Toolbar label="Filters">
        <input aria-label="Search" />
        <button>Apply</button>
      </Toolbar>,
    );
    // `role="toolbar"` tells a screen-reader user that arrow keys move between
    // the controls. Here they belong to the caret in the search field, so the
    // role would be a promise the component does not keep.
    expect(screen.queryByRole("toolbar")).toBeNull();
    expect(screen.getByRole("group", { name: "Filters" })).toBeDefined();
    expect(screen.getByLabelText("Search").tabIndex).toBe(0);
  });

  test("a cluster survives the wrap as one control", () => {
    formatting();
    // Not a styling detail: once a row wraps, a segmented control split across
    // two lines stops reading as one control, and the group is what holds it
    // together.
    expect(screen.getByRole("group", { name: "Text style" })).toBeDefined();
  });
});

describe("a toolbar of buttons is one tab stop", () => {
  test("only the first control is in the tab order", () => {
    formatting();
    // mail's formatting bar is a dozen icon buttons, which was a dozen tab
    // stops between the message list and the message body.
    expect(screen.getByRole("toolbar", { name: "Formatting" })).toBeDefined();
    expect(stops()).toEqual([0, -1, -1]);
  });

  test("arrow keys move between the controls, and wrap round", () => {
    formatting();
    const toolbar = screen.getByRole("toolbar", { name: "Formatting" });
    screen.getByText("Bold").focus();

    fireEvent.keyDown(toolbar, { key: "ArrowRight" });
    expect(document.activeElement).toBe(screen.getByText("Italic"));
    fireEvent.keyDown(toolbar, { key: "ArrowRight" });
    fireEvent.keyDown(toolbar, { key: "ArrowRight" });
    expect(document.activeElement).toBe(screen.getByText("Bold"));
    fireEvent.keyDown(toolbar, { key: "ArrowLeft" });
    expect(document.activeElement).toBe(screen.getByText("Link"));
  });

  test("Home and End reach the ends without scrolling the page", () => {
    formatting();
    const toolbar = screen.getByRole("toolbar", { name: "Formatting" });
    screen.getByText("Italic").focus();

    const end = fireEvent.keyDown(toolbar, { key: "End" });
    expect(document.activeElement).toBe(screen.getByText("Link"));
    // Not prevented, End scrolls the pane under the toolbar instead.
    expect(end).toBe(false);

    fireEvent.keyDown(toolbar, { key: "Home" });
    expect(document.activeElement).toBe(screen.getByText("Bold"));
  });

  test("the tab stop follows the control last used", () => {
    formatting();
    screen.getByText("Link").focus();
    // Tabbing out and back returns to where you were, rather than to the start
    // of the row — the point of a roving tab stop rather than a fixed one.
    expect(stops()).toEqual([-1, -1, 0]);
  });

  test("a control that appears later arrives outside the tab order", () => {
    const { rerender } = render(
      <Toolbar label="Selection" keyboard="roving">
        <button>Select all</button>
      </Toolbar>,
    );
    rerender(
      <Toolbar label="Selection" keyboard="roving">
        <button>Select all</button>
        <button>Delete</button>
      </Toolbar>,
    );
    // A button that shows up when a row is selected would otherwise arrive with
    // the default tabIndex of 0 and quietly make the single tab stop two.
    expect(stops()).toEqual([0, -1]);
  });

  test("a disabled control is skipped rather than focused", () => {
    render(
      <Toolbar label="Selection" keyboard="roving">
        <button>Select all</button>
        <button disabled>Delete</button>
        <button>Export</button>
      </Toolbar>,
    );
    const toolbar = screen.getByRole("toolbar", { name: "Selection" });
    screen.getByText("Select all").focus();
    fireEvent.keyDown(toolbar, { key: "ArrowRight" });
    expect(document.activeElement).toBe(screen.getByText("Export"));
  });
});
