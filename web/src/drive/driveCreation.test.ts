import { describe, expect, test } from "vitest";

import { nextUntitledName } from "./driveCreation";

describe("nextUntitledName", () => {
  test("uses the localized base name when it is free", () => {
    expect(nextUntitledName("Untitled document", ["Project plan"])).toBe(
      "Untitled document",
    );
  });

  test("chooses the next free suffix without overwriting a visible document", () => {
    expect(
      nextUntitledName("Untitled document", [
        "Untitled document",
        "Untitled document 2",
        "Untitled document 4",
      ]),
    ).toBe("Untitled document 3");
  });

  test("compares names without case sensitivity", () => {
    expect(nextUntitledName("Naamloos document", ["naamloos document"])).toBe(
      "Naamloos document 2",
    );
  });
});
