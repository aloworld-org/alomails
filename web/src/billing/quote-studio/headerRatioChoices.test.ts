import { describe, expect, it } from "vitest";

import { HEADER_RATIO_CHOICES } from "./headerRatioChoices";

describe("header ratio choices", () => {
  it("provides normal and reversed layouts for each ratio", () => {
    expect(HEADER_RATIO_CHOICES).toHaveLength(3);
    expect(
      HEADER_RATIO_CHOICES.every(
        (choice) => choice.columns && choice.reverseColumns,
      ),
    ).toBe(true);
  });
});
