import { describe, expect, it } from "vitest";

import { driveErrorReason, driveNodeOrigin } from "./parts";

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

describe("driveNodeOrigin", () => {
  it("reads the source a file was saved from", () => {
    expect(driveNodeOrigin({ sourceKind: "email", sourceId: "msg-1" })).toEqual({
      kind: "message",
      id: "msg-1",
      label: null,
    });
    expect(driveNodeOrigin({ sourceKind: "chat", sourceId: "room-1" })).toEqual({
      kind: "thread",
      id: "room-1",
      label: null,
    });
    expect(driveNodeOrigin({ sourceKind: "event", sourceId: "ev-1" })).toEqual({
      kind: "event",
      id: "ev-1",
      label: null,
    });
  });

  it("says nothing when the node names no source", () => {
    expect(driveNodeOrigin({ sourceKind: null, sourceId: null })).toBeNull();
    // A kind with nothing to point at is not a citation anybody can follow.
    expect(driveNodeOrigin({ sourceKind: "email", sourceId: null })).toBeNull();
    expect(driveNodeOrigin({ sourceKind: "elsewhere", sourceId: "x" })).toBeNull();
  });
});
