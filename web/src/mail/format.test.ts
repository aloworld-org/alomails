import { describe, expect, it } from "vitest";

import { strings } from "../i18n";
import type { EmailFull, EmailHeaders } from "../jmap";
import { formatDate, isUnread, mailErrorReason, recipientName, senderName, subjectOr } from "./format";
import { sandboxedHtml, textContent } from "./body";

function headers(partial: Partial<EmailHeaders>): EmailHeaders {
  return {
    id: "e1",
    threadId: "t1",
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

describe("recipientName", () => {
  // The bug this function exists for: a Sent list rendered with
  // senderName is a column of the account owner's own name, so a message
  // that was sent, delivered and stored looks like it never happened.
  it("shows who the message went to, not who sent it", () => {
    const row = headers({
      from: [{ name: "disan", email: "disan@alomails.com" }],
      to: [{ name: "Kevin", email: "kevin.impens@axongroup.com" }],
    });
    expect(recipientName(row)).toBe("Kevin");
    expect(recipientName(row)).not.toBe(senderName(row));
  });

  it("falls back to the address when the recipient has no display name", () => {
    const row = headers({ to: [{ name: null, email: "kevin.impens@axongroup.com" }] });
    expect(recipientName(row)).toBe("kevin.impens@axongroup.com");
  });

  it("uses cc when a message was addressed only by carbon copy", () => {
    const row = headers({ to: [], cc: [{ name: "Copied", email: "cc@example.test" }] });
    expect(recipientName(row)).toBe("Copied");
  });

  // A draft with nobody in the To field is an ordinary state, not an error.
  it("names the empty case rather than rendering nothing", () => {
    expect(recipientName(headers({}))).toBe(strings.mailNoRecipient);
  });
});

describe("senderName", () => {
  it("prefers the display name", () => {
    expect(senderName(headers({ from: [{ name: "Alice Ng", email: "a@x.eu" }] }))).toBe("Alice Ng");
  });
  it("falls back to the email when the name is blank", () => {
    expect(senderName(headers({ from: [{ name: "  ", email: "a@x.eu" }] }))).toBe("a@x.eu");
  });
  it("uses the unknown-sender label when there is no sender", () => {
    expect(senderName(headers({ from: null }))).toBe(strings.mailUnknownSender);
  });
});

describe("subjectOr", () => {
  it("returns the subject when present", () => {
    expect(subjectOr(headers({ subject: "Invoice" }))).toBe("Invoice");
  });
  it("returns the placeholder for an empty subject", () => {
    expect(subjectOr(headers({ subject: "" }))).toBe(strings.mailNoSubject);
    expect(subjectOr(headers({ subject: null }))).toBe(strings.mailNoSubject);
  });
});

describe("isUnread", () => {
  it("is unread without the $seen keyword", () => {
    expect(isUnread(headers({ keywords: {} }))).toBe(true);
  });
  it("is read with the $seen keyword", () => {
    expect(isUnread(headers({ keywords: { $seen: true } }))).toBe(false);
  });
});

describe("formatDate", () => {
  const now = new Date("2026-07-01T18:00:00Z");
  it("shows a time for the same day", () => {
    expect(formatDate("2026-07-01T09:30:00Z", now)).toMatch(/\d/);
  });
  it("omits the year within the same year", () => {
    expect(formatDate("2026-02-14T09:30:00Z", now)).not.toMatch(/2026/);
  });
  it("includes the year for older messages", () => {
    expect(formatDate("2025-12-14T09:30:00Z", now)).toMatch(/2025/);
  });
  it("returns empty for an unparseable date", () => {
    expect(formatDate("not-a-date", now)).toBe("");
  });
});

describe("mailErrorReason", () => {
  it("preserves a server error message", () => {
    expect(mailErrorReason(new Error("Mailbox quota exceeded"))).toBe("Mailbox quota exceeded");
  });

  it("accepts a non-empty string reason", () => {
    expect(mailErrorReason("Attachment type is blocked")).toBe("Attachment type is blocked");
  });

  it("ignores values that do not explain the failure", () => {
    expect(mailErrorReason(new Error("  "))).toBeNull();
    expect(mailErrorReason({ status: 500 })).toBeNull();
  });
});

describe("body", () => {
  function full(): EmailFull {
    return {
      ...headers({}),
      textBody: [{ partId: "1", type: "text/plain" }],
      htmlBody: [],
      bodyValues: { "1": { value: "hello world", isTruncated: false } },
      attachments: [],
    };
  }
  it("extracts the plain-text body", () => {
    expect(textContent(full())).toBe("hello world");
  });
  it("wraps html with a content-security policy that blocks remote content", () => {
    const doc = sandboxedHtml("<p>hi</p>");
    expect(doc).toContain("Content-Security-Policy");
    expect(doc).toContain("default-src 'none'");
    expect(doc).toContain("<p>hi</p>");
  });
});
