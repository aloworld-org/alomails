import { describe, expect, it } from "vitest";
import * as subject from "./SchedulesView";

describe("SchedulesView", () => {
  it("exports its public component API", () => {
    expect(Object.keys(subject).length).toBeGreaterThan(0);
  });
});
