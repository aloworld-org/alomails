import { describe, expect, it } from "vitest";
import * as subject from "./EditorChoice";

describe("EditorChoice", () => {
  it("exports its public component API", () => {
    expect(Object.keys(subject).length).toBeGreaterThan(0);
  });
});
