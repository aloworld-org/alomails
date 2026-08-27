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
