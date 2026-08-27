import { describe, expect, it } from "vitest";
import * as subject from "./ErrorBanner";

describe("ErrorBanner", () => {
  it("exports its public component API", () => {
    expect(Object.keys(subject).length).toBeGreaterThan(0);
  });
});
