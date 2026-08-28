import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { ListBlockContent } from "./ListBlockContent";

afterEach(cleanup);

describe("ListBlockContent", () => {
  it("numbers nested items with the block's style and skips empty lines", () => {
    render(
      <ListBlockContent
        block={{ id: "l", kind: "list", ordered: true, items: "Scope\n\tDesign\n\tBuild\n\nHandover", style: "outline" }}
      />,
    );
    const rows = screen.getAllByRole("listitem");
    expect(rows.map((row) => row.textContent)).toEqual([
      "1.Scope",
      "1.1.Design",
      "1.2.Build",
      "2.Handover",
    ]);
    expect(rows[1]?.className).toContain("pl-6");
  });

  it("renders a design saved before styles existed with plain numbers", () => {
    render(
      <ListBlockContent block={{ id: "l", kind: "list", ordered: true, items: "One\nTwo" }} />,
    );
    expect(screen.getAllByRole("listitem").map((row) => row.textContent)).toEqual(["1.One", "2.Two"]);
  });
});
