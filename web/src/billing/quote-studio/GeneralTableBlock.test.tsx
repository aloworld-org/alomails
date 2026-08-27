import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { GeneralTableBlock } from "./GeneralTableBlock";

describe("GeneralTableBlock", () => {
  it("renders a localized empty editor", () => {
    render(<GeneralTableBlock block={{ id: "table", kind: "table", columns: [{ id: "one", label: "Name" }], rows: [] }} readOnly={false} onChange={vi.fn()} />);
    expect(screen.getByRole("button", { name: /add row/i })).toBeTruthy();
  });
});
