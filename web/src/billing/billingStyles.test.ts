import { describe, expect, it } from "vitest";

import styles from "./billingStyles";

describe("billing document chrome", () => {
  it("uses hierarchy and surface changes instead of nested panel borders", () => {
    expect(styles.editor).not.toMatch(/\bborder\b/);
    expect(styles.editorHead).not.toContain("border-b");
    expect(styles.documentSummary).not.toMatch(/\bborder\b/);
    expect(styles.documentNote).not.toContain("border-t");
  });
});
