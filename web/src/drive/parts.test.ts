import { describe, expect, it } from "vitest";

import { driveErrorReason } from "./parts";

describe("driveErrorReason", () => {
  it("keeps a backend Error message", () => {
    expect(driveErrorReason(new Error("Storage quota exceeded"))).toBe("Storage quota exceeded");
  });

  it("keeps a string reason", () => {
    expect(driveErrorReason("Upload type is blocked")).toBe("Upload type is blocked");
  });

  it("rejects empty and opaque values", () => {
    expect(driveErrorReason(new Error("  "))).toBeNull();
    expect(driveErrorReason({ status: 500 })).toBeNull();
  });
});
