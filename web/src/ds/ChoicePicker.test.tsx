import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { ChoicePicker } from "./ChoicePicker";

const options = [
  { value: "first", label: "First option" },
  { value: "second", label: "Second option" },
  { value: "disabled", label: "Disabled option", disabled: true },
];

afterEach(cleanup);

describe("ChoicePicker", () => {
  test("opens a branded listbox and chooses an enabled option", () => {
    const onChange = vi.fn();
    render(
      <ChoicePicker
        value="first"
        options={options}
        placeholder="Choose"
        label="Example choice"
        onChange={onChange}
      />,
    );

    fireEvent.click(screen.getByRole("combobox", { name: "Example choice" }));
    expect(screen.getByRole("listbox", { name: "Example choice" })).toBeTruthy();
    fireEvent.click(screen.getByRole("option", { name: "Second option" }));

    expect(onChange).toHaveBeenCalledWith("second");
    expect(screen.queryByRole("listbox", { name: "Example choice" })).toBeNull();
  });

  test("keeps the pointer highlight and the chosen value visually distinct", () => {
    render(
      <ChoicePicker
        value="second"
        options={options}
        placeholder="Choose"
        label="Example choice"
        onChange={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("combobox", { name: "Example choice" }));
    const first = screen.getByRole("option", { name: "First option" });
    const second = screen.getByRole("option", { name: "Second option" });
    fireEvent.mouseEnter(first);

    // The fill marks the one row the pointer rests on; the accent colour and
    // the check mark mark the value. Neither row shows the other's cue, so
    // two rows never read as chosen at once.
    expect(first.className).toContain("!bg-raised");
    expect(second.className).not.toContain("!bg-raised");
    expect(second.className).toContain("!text-accent");
    expect(first.className).not.toContain("!text-accent");
    expect(second.getAttribute("aria-selected")).toBe("true");
    expect(first.getAttribute("aria-selected")).toBe("false");
    expect(second.querySelector("svg")).toBeTruthy();
    expect(first.querySelector("svg")).toBeNull();
  });

  test("supports keyboard movement and never selects a disabled option", () => {
    const onChange = vi.fn();
    render(
      <ChoicePicker
        value="first"
        options={options}
        placeholder="Choose"
        label="Example choice"
        onChange={onChange}
      />,
    );

    const trigger = screen.getByRole("combobox", { name: "Example choice" });
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith("second");

    fireEvent.click(trigger);
    const disabled = screen.getByRole("option", { name: "Disabled option" });
    expect((disabled as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(disabled);
    expect(onChange).toHaveBeenCalledTimes(1);
  });
});
