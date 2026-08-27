// The server-sync seam (mail M4.2): adopting the stored choice at sign-in
// must apply locally without echoing a write back, and a switcher change
// must reach the registered writer exactly when a session is live.
import { afterEach, describe, expect, test, vi } from "vitest";

import {
  adoptRemoteLocale,
  getLocale,
  setLocale,
  setRemoteLocaleWriter,
} from "./locale";

afterEach(() => {
  setRemoteLocaleWriter(null);
  setLocale("en");
});

describe("adopting the server preference", () => {
  test("a stored choice applies locally", () => {
    adoptRemoteLocale("de");
    expect(getLocale()).toBe("de");
  });

  test("null means never chosen — the detected locale stands", () => {
    setLocale("fr");
    adoptRemoteLocale(null);
    expect(getLocale()).toBe("fr");
  });

  test("a tag this bundle does not ship is ignored, not an error", () => {
    adoptRemoteLocale("pt-BR");
    expect(getLocale()).toBe("en");
  });

  test("adoption never writes back to the server", () => {
    const writer = vi.fn();
    setRemoteLocaleWriter(writer);
    adoptRemoteLocale("nl");
    expect(getLocale()).toBe("nl");
    // The value came FROM the server; echoing it back would make every
    // sign-in a write.
    expect(writer).not.toHaveBeenCalled();
  });
});

describe("writing a switch through", () => {
  test("a switcher change reaches the registered writer", () => {
    const writer = vi.fn();
    setRemoteLocaleWriter(writer);
    setLocale("de");
    expect(writer).toHaveBeenCalledWith("de");
  });

  test("switching to the already-active language writes nothing", () => {
    const writer = vi.fn();
    setRemoteLocaleWriter(writer);
    setLocale("en");
    expect(writer).not.toHaveBeenCalled();
  });

  test("with no writer registered (anonymous pages) a switch stays local", () => {
    // Simply must not throw — the choice still applies.
    setLocale("nl");
    expect(getLocale()).toBe("nl");
  });

  test("a removed writer (sign-out) is no longer called", () => {
    const writer = vi.fn();
    setRemoteLocaleWriter(writer);
    setRemoteLocaleWriter(null);
    setLocale("fr");
    expect(writer).not.toHaveBeenCalled();
  });
});
