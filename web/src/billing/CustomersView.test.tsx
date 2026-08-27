import { describe, expect, it } from "vitest";
import * as subject from "./CustomersView";

describe("CustomersView", () => {
  it("exports its public component API", () => {
    expect(Object.keys(subject).length).toBeGreaterThan(0);
  });
});
