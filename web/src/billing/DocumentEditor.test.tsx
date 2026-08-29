import { describe, expect, it } from "vitest";
import * as subject from "./DocumentEditor";

describe("DocumentEditor", () => {
  it("exports its public component API", () => {
    expect(Object.keys(subject).length).toBeGreaterThan(0);
  });

  it("can let a document-specific editor overlay escape its shell", () => {
    expect(subject.documentEditorClass(false)).not.toContain("!overflow-visible");
    expect(subject.documentEditorClass(true)).toContain("!overflow-visible");
  });
});
