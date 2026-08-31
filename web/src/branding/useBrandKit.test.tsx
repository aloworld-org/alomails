import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, test } from "vitest";

import { useBrandKit } from "./useBrandKit";

beforeEach(() => {
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: {
      length: 0,
      clear: () => undefined,
      getItem: () => null,
      key: () => null,
      removeItem: () => undefined,
      setItem: () => { throw new Error("quota exceeded"); },
    } satisfies Storage,
  });
});

test("brand kit keeps the draft and reports a failed persistence write", () => {
  const { result } = renderHook(() => useBrandKit());
  act(() => result.current.setDraft({ ...result.current.draft, foundation: { ...result.current.draft.foundation, name: "Northstar" } }));
  act(() => result.current.save());
  expect(result.current.draft.foundation.name).toBe("Northstar");
  expect(result.current.saveFailed).toBe(true);
  expect(result.current.dirty).toBe(true);
});
