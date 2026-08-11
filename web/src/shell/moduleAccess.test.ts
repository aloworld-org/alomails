// The per-user app switches, as the client reads them (migration 0208).
//
// The rule under test is the *failure direction*. This file decides what the
// rail shows, and the server decides what anybody can actually reach — so
// every uncertain case here must resolve toward showing the app, never toward
// hiding it. Hiding on doubt would empty somebody's whole workspace because
// one fetch was slow.
import { describe, expect, test } from "vitest";

import { isModuleAllowed } from "./moduleAccess";

describe("what the rail offers", () => {
  test("an app nobody switched off is offered", () => {
    expect(isModuleAllowed(new Set(), "billing")).toBe(true);
  });

  test("an app the admin switched off is not offered", () => {
    expect(isModuleAllowed(new Set(["billing"]), "billing")).toBe(false);
  });

  test("switching one app off leaves the others alone", () => {
    const denied = new Set(["billing", "crm"]);
    expect(isModuleAllowed(denied, "drive")).toBe(true);
    expect(isModuleAllowed(denied, "crm")).toBe(false);
  });

  test("unknown reads as allowed, so the rail never blinks empty", () => {
    // `null` is "still asking", which is not "denied". Treating the two the
    // same would draw an empty rail on the first paint of every session and
    // fill it in afterwards — for the benefit of the rare person who is
    // denied anything at all.
    expect(isModuleAllowed(null, "billing")).toBe(true);
    expect(isModuleAllowed(null, "anything")).toBe(true);
  });
});
