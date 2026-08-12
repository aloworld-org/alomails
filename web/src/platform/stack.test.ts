// The stack marker's one dangerous property: it must never appear on a
// deployed origin. A badge on production would leak internal topology to
// customers, and — the subtler harm — would train the people who need it to
// stop seeing it.
import { describe, expect, test } from "vitest";

import { stackLabel } from "./stack";

describe("which stack is on screen", () => {
  test("a deployed origin is never labelled", () => {
    for (const origin of [
      "https://app.aloworkplace.com",
      "https://mail.alomails.com",
      "https://aloworkplace.com",
    ]) {
      expect(stackLabel(origin, origin)).toBeNull();
    }
  });

  test("a local dev server is labelled by its port", () => {
    // The case that cost an evening: two dev servers, identical on screen.
    expect(stackLabel("http://localhost:5173", "http://localhost:5173")?.text).toBe(
      "dev :5173",
    );
    expect(stackLabel("http://localhost:5174", "http://localhost:5174")?.text).toBe(
      "dev :5174",
    );
  });

  test("a cross-origin API is named, because that is the confusing case", () => {
    // The desktop app, and any dev server pointed at a backend directly.
    const label = stackLabel("http://localhost:5174", "http://localhost:8080");
    expect(label?.text).toBe("dev :5174 → :8080");
    expect(label?.detail).toContain("http://localhost:8080");
  });

  test("127.0.0.1 and ::1 count as local, since people type them", () => {
    expect(stackLabel("http://127.0.0.1:5173", "http://127.0.0.1:5173")).not.toBeNull();
    expect(stackLabel("http://[::1]:5173", "http://[::1]:5173")).not.toBeNull();
  });

  test("a local page pointed at production is still labelled", () => {
    // The desktop build during development talks to the hosted server. That is
    // the single most dangerous combination to mistake for production, so the
    // page being local is what decides, not the API.
    const label = stackLabel("http://localhost:5173", "https://mail.alomails.com");
    expect(label?.text).toBe("dev :5173 → :443");
    expect(label?.detail).toContain("mail.alomails.com");
  });

  test("nonsense input shows nothing rather than throwing", () => {
    expect(stackLabel("", "")).toBeNull();
    expect(stackLabel("not-a-url", "http://localhost:5173")).toBeNull();
  });
});
