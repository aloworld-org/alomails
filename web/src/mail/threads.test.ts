import { describe, expect, it } from "vitest";

import type { EmailHeaders } from "../jmap";
import { groupThreads } from "./threads";

function email(partial: Partial<EmailHeaders> & { id: string; threadId: string }): EmailHeaders {
  return {
    blobId: "blob1",
    mailboxIds: {},
    keywords: {},
    from: null,
    to: null,
    cc: null,
    bcc: null,
    subject: null,
    receivedAt: "2026-07-01T09:00:00Z",
    size: 0,
    preview: "",
    hasAttachment: false,
    messageId: null,
    references: null,
    ...partial,
  };
}

describe("groupThreads", () => {
  it("collapses a thread's messages into one row with a count", () => {
    const rows = groupThreads(
      [
        email({ id: "a", threadId: "t1", receivedAt: "2026-07-01T08:00:00Z" }),
        email({ id: "b", threadId: "t1", receivedAt: "2026-07-01T10:00:00Z" }),
        email({ id: "c", threadId: "t2", receivedAt: "2026-07-01T09:00:00Z" }),
      ],
      new Set(),
      new Map(),
    );
    expect(rows).toHaveLength(2);
    const t1 = rows.find((r) => r.threadId === "t1");
    expect(t1?.count).toBe(2);
    expect(t1?.latest.id).toBe("b"); // the newest message
    expect(t1?.memberIds).toEqual(["a", "b"]);
  });

  it("orders threads by their newest message, newest first", () => {
    const rows = groupThreads(
      [
        email({ id: "a", threadId: "t1", receivedAt: "2026-07-01T08:00:00Z" }),
        email({ id: "c", threadId: "t2", receivedAt: "2026-07-01T12:00:00Z" }),
      ],
      new Set(),
      new Map(),
    );
    expect(rows.map((r) => r.threadId)).toEqual(["t2", "t1"]);
  });

  it("marks a thread unread if any member is unread and not locally read", () => {
    const rows = groupThreads(
      [
        email({ id: "a", threadId: "t1", keywords: { $seen: true } }),
        email({ id: "b", threadId: "t1", keywords: {} }),
      ],
      new Set(),
      new Map(),
    );
    expect(rows[0]?.hasUnread).toBe(true);
  });

  it("treats a locally-read member as read", () => {
    const rows = groupThreads(
      [email({ id: "b", threadId: "t1", keywords: {} })],
      new Set(["b"]),
      new Map(),
    );
    expect(rows[0]?.hasUnread).toBe(false);
  });
});
