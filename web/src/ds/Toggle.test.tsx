// What seven hand-rolled toggles did not do.
//
// The geometry was never in dispute — the two that drew a switch had arrived at
// the same 16px of travel independently. Everything tested here is a property
// that was missing from all of them, and every one is invisible until the
// screen is used with something other than a mouse.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { Checkbox } from "./Checkbox";
import { Toggle } from "./Toggle";

afterEach(cleanup);

describe("a toggle is a switch, and says so", () => {
  test("it is announced as a switch that is on, not a box that is checked", () => {
    render(<Toggle checked onChange={() => {}} label="Out of office" />);
    // Both copies were plain checkboxes, so "out of office" was read as
    // "checked" — a state, but not this control's state.
    const control = screen.getByRole("switch", { name: "Out of office" });
    expect(control.getAttribute("type")).toBe("checkbox");
    expect((control as HTMLInputElement).checked).toBe(true);
  });

  test("the label is bound to the control, not merely beside it", () => {
    render(<Toggle checked={false} onChange={() => {}} label="Admin access" />);
    const control = screen.getByRole("switch") as HTMLInputElement;
    // `admin/UserModal` puts the visible text in a sibling `<span>` and the
    // name in `aria-label` on the `<label>`; `admin/UsersPage` uses `title`,
    // which is a tooltip. Clicking the word did nothing in either.
    expect(control.labels?.length).toBe(1);
    expect(control.labels?.[0]?.textContent).toBe("Admin access");
  });

  test("a hidden label is still the name", () => {
    render(
      <Toggle checked={false} onChange={() => {}} label="Admin" hideLabel />,
    );
    // The shape `admin/UsersPage` needs: a switch in a table row whose column
    // header already says what it does. Hidden is not the same as absent.
    expect(screen.getByRole("switch", { name: "Admin" })).toBeDefined();
    expect(screen.getByText("Admin")).toBeDefined();
  });

  test("the hint is described, not just drawn", () => {
    render(
      <Toggle
        checked={false}
        onChange={() => {}}
        label="Out of office"
        hint="Replies are sent once per sender."
      />,
    );
    const control = screen.getByRole("switch");
    const describedBy = control.getAttribute("aria-describedby");
    expect(describedBy).not.toBeNull();
    expect(document.getElementById(describedBy ?? "")?.textContent).toBe(
      "Replies are sent once per sender.",
    );
  });

  test("it reports the new state, so a caller never re-derives it", () => {
    const seen: boolean[] = [];
    render(
      <Toggle checked={false} onChange={(v) => seen.push(v)} label="Enabled" />,
    );
    fireEvent.click(screen.getByRole("switch"));
    expect(seen).toEqual([true]);
  });

  test("a disabled switch shows that it refuses", () => {
    const { container } = render(
      <Toggle
        checked={false}
        onChange={() => {}}
        label="Admin access"
        disabled
      />,
    );
    // The refusal itself is the platform's — `disabled` on the control, which
    // both copies already set on real conditions (your own admin row, an
    // unconfigured provider). What neither did was draw it: the track and the
    // cursor were identical to a live switch, so a control that would not move
    // was indistinguishable from one that was broken.
    expect((screen.getByRole("switch") as HTMLInputElement).disabled).toBe(
      true,
    );
    expect(container.firstElementChild?.className).toContain("disabled");
  });

  test("the drawn track is not announced twice", () => {
    const { container } = render(
      <Toggle checked onChange={() => {}} label="Enabled" />,
    );
    expect(container.querySelectorAll("[aria-hidden='true']").length).toBe(1);
    expect(screen.getAllByRole("switch").length).toBe(1);
  });
});

describe("a checkbox is one option among several", () => {
  test("its label is its own, and bound", () => {
    render(
      <Checkbox checked={false} onChange={() => {}} label="Include archived" />,
    );
    // `billing/ProductDialog` and `shell/SettingsModal` put this row's class on
    // a `<span>`, so the word beside the box was attached to nothing.
    const box = screen.getByRole("checkbox", {
      name: "Include archived",
    }) as HTMLInputElement;
    expect(box.labels?.length).toBe(1);
  });

  test("it is a checkbox, not a switch", () => {
    render(<Checkbox checked onChange={() => {}} label="Only mine" />);
    // The distinction the copies had lost: a filter beside a search field is
    // not a setting, and it should not be announced as one.
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.getByRole("checkbox", { name: "Only mine" })).toBeDefined();
  });

  test("a disabled box shows that it refuses", () => {
    const { container } = render(
      <Checkbox
        checked={false}
        onChange={() => {}}
        label="Stocked only"
        disabled
      />,
    );
    // None of the four had a disabled state at all.
    expect((screen.getByRole("checkbox") as HTMLInputElement).disabled).toBe(
      true,
    );
    expect(container.firstElementChild?.className).toContain("disabled");
  });

  test("it reports the new state", () => {
    const seen: boolean[] = [];
    render(
      <Checkbox
        checked={false}
        onChange={(v) => seen.push(v)}
        label="Include archived"
      />,
    );
    fireEvent.click(screen.getByRole("checkbox"));
    expect(seen).toEqual([true]);
  });
});
