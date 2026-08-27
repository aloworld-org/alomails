import { describe, expect, it } from "vitest";
import * as subject from "./SharePricesDialog";

describe("SharePricesDialog", () => {
  it("exports its public component API", () => {
    expect(Object.keys(subject).length).toBeGreaterThan(0);
  });
});
