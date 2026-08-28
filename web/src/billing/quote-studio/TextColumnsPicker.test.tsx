import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { strings } from "../../i18n";
import {
  TextColumnsPicker,
  textColumns,
  textColumnsClass,
} from "./TextColumnsPicker";

describe("textColumns", () => {
  it("reads any saved value as a valid count, defaulting to one", () => {
    // A design saved before the setting existed carries no value at all; one
    // hand-edited in storage can carry anything. Both must render.
    expect(textColumns(undefined)).toBe(1);
    expect(textColumns(2)).toBe(2);
    expect(textColumns(3)).toBe(3);
    expect(textColumns(4)).toBe(1);
    expect(textColumns("2")).toBe(1);
  });
});

describe("textColumnsClass", () => {
  it("adds nothing for one column, so an untouched block renders as before", () => {
    expect(textColumnsClass(1)).toBe("");
  });

  it("flows two and three columns and keeps each paragraph whole", () => {
    expect(textColumnsClass(2)).toContain("md:columns-2");
    expect(textColumnsClass(3)).toContain("md:columns-3");
    expect(textColumnsClass(2)).toContain("break-inside-avoid");
  });
});

describe("TextColumnsPicker", () => {
  it("offers one to three columns and reports the choice as a number", () => {
    const onChange = vi.fn();
    render(
      <TextColumnsPicker
        value={1}
        label={strings.quoteStudioParagraphColumns}
        onChange={onChange}
      />,
    );
    fireEvent.click(
      screen.getByRole("combobox", { name: strings.quoteStudioParagraphColumns }),
    );
    fireEvent.click(
      screen.getByRole("option", { name: strings.quoteStudioColumnCount(3) }),
    );
    expect(onChange).toHaveBeenCalledWith(3);
  });
});
