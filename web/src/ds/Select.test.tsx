// What seven hand-rolled selects did not do.
//
// The box around a native select is the easy half and the copies had already
// agreed on it. What they disagreed about, or forgot entirely, is what a
// screen reader is given and what the empty option means — neither of which is
// visible by looking at the screen.
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { Field } from "./Field";
import { Select } from "./Select";

afterEach(cleanup);

function locations() {
  return (
    <>
      <option value="ams">Amsterdam</option>
      <option value="ber">Berlin</option>
    </>
  );
}

describe("the select is named, and its empty option means something", () => {
  test("a wrapping label names it, with no aria plumbing", () => {
    render(
      <label>
        Location
        <Select defaultValue="ams">{locations()}</Select>
      </label>,
    );
    // The pattern five of the seven call sites already use. It has to keep
    // working, or migrating them would mean rewriting their markup as well.
    expect(screen.getByRole("combobox", { name: "Location" })).toBeDefined();
  });

  test("a Field names it, describes it and marks it invalid", () => {
    render(
      <Field label="Location" hint="Where the stock sits" error="Pick one">
        {(control) => (
          <Select {...control} defaultValue="ams">
            {locations()}
          </Select>
        )}
      </Field>,
    );
    const select = screen.getByRole("combobox", { name: "Location" });
    // The three wires that make a labelled control: the label points at the
    // id, the hint and the error are read with it, and the error state reaches
    // assistive technology rather than only the border.
    expect(select.getAttribute("aria-invalid")).toBe("true");
    const described = select.getAttribute("aria-describedby") ?? "";
    expect(described.split(" ").length).toBe(2);
    expect(screen.getByText("Where the stock sits").id).toBe(
      described.split(" ")[0],
    );
    expect(screen.getByRole("alert").id).toBe(described.split(" ")[1]);
  });

  test("a Field can keep long guidance in an accessible information control", () => {
    const hint = "The country determines VAT treatment.";
    render(
      <Field label="Country" hint={hint} hintDisplay="tooltip">
        {(control) => (
          <Select {...control} defaultValue="ams">
            {locations()}
          </Select>
        )}
      </Field>,
    );

    const select = screen.getByRole("combobox", { name: "Country" });
    expect(screen.getByRole("button", { name: hint })).toBeDefined();
    expect(select.getAttribute("aria-describedby")).not.toBeNull();
  });

  test("an unnamed select is reported in development", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(<Select defaultValue="ams">{locations()}</Select>);
    // `shell/FiltersSection` ships two of these. Nothing about a hand-rolled
    // select could have said so; this is the whole reason the check exists.
    expect(spy).toHaveBeenCalledOnce();
    expect(spy.mock.calls[0]?.[0]).toContain("no accessible name");
    spy.mockRestore();
  });

  test("a named select says nothing", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(
      <Select aria-label="Location" defaultValue="ams">
        {locations()}
      </Select>,
    );
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  test("the placeholder is first, empty, and what is shown before a choice", () => {
    render(
      <Select
        aria-label="Location"
        placeholder="All locations"
        value=""
        onChange={() => {}}
      >
        {locations()}
      </Select>,
    );
    const select = screen.getByRole<HTMLSelectElement>("combobox");
    const options = screen.getAllByRole<HTMLOptionElement>("option");
    expect(options[0]?.textContent).toBe("All locations");
    // `value=""` and not a sentinel: it is the value the browser's own required
    // check treats as "nothing chosen", and the value six call sites already
    // compare against.
    expect(options[0]?.value).toBe("");
    expect(select.value).toBe("");
  });

  test("where empty is a real answer, the prompt stays choosable", () => {
    render(
      <Select
        aria-label="Location"
        placeholder="All locations"
        value=""
        onChange={() => {}}
      >
        {locations()}
      </Select>,
    );
    // inventory's "All locations" and billing's cleared product picker are
    // states somebody has to be able to get back to.
    expect(screen.getAllByRole<HTMLOptionElement>("option")[0]?.disabled).toBe(
      false,
    );
  });

  test("on a required select the prompt cannot be chosen again", () => {
    render(
      <Select
        aria-label="Location"
        placeholder="Pick a location"
        required
        defaultValue=""
      >
        {locations()}
      </Select>,
    );
    const prompt = screen.getAllByRole<HTMLOptionElement>("option")[0];
    // It still shows as the current selection — the question is visible until
    // it is answered — but it is out of reach once it has been.
    expect(prompt?.disabled).toBe(true);
    expect(prompt?.value).toBe("");
  });
});
