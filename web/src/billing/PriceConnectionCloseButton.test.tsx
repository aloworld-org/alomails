import { describe, expect, it } from "vitest";
import * as subject from "./PriceConnectionCloseButton";

describe("PriceConnectionCloseButton", () => {
  it("exports its public component API", () => {
    expect(Object.keys(subject).length).toBeGreaterThan(0);
  });
});
