import { describe, expect, it, vi } from "vitest";

import type { EmailFull } from "../../jmap";
import { buildPrefill, materializeAttachments, replyAllRecipients } from "./ComposeModal";

const addr = (email: string) => ({ name: null, email });

describe("replyAllRecipients", () => {
  it("puts the sender and all original To recipients in To, minus me", () => {
    const source = {
      from: [addr("steve@rebuild.com")],
      to: [addr("me@alo.dev"), addr("adam@gmail.com")],
      cc: null,
    };
    const { to, cc } = replyAllRecipients(source, "me@alo.dev");
    expect(to.map((a) => a.email)).toEqual(["steve@rebuild.com", "adam@gmail.com"]);
    expect(cc).toEqual([]);
  });

  it("keeps the original Cc, excluding me and anyone already in To", () => {
    const source = {
      from: [addr("steve@rebuild.com")],
      to: [addr("adam@gmail.com")],
      cc: [addr("me@alo.dev"), addr("adam@gmail.com"), addr("lena@rebuild.com")],
    };
    const { to, cc } = replyAllRecipients(source, "me@alo.dev");
    expect(to.map((a) => a.email)).toEqual(["steve@rebuild.com", "adam@gmail.com"]);
    expect(cc.map((a) => a.email)).toEqual(["lena@rebuild.com"]);
  });

  it("dedupes case-insensitively and ignores empty addresses", () => {
    const source = {
      from: [addr("Steve@Rebuild.com")],
      to: [addr("steve@rebuild.com"), addr(""), addr("adam@gmail.com")],
      cc: null,
    };
    const { to } = replyAllRecipients(source, "me@alo.dev");
    expect(to.map((a) => a.email)).toEqual(["Steve@Rebuild.com", "adam@gmail.com"]);
  });

  it("excludes me even when I am the sender (reply to my own message)", () => {
    const source = {
      from: [addr("me@alo.dev")],
      to: [addr("adam@gmail.com")],
      cc: null,
    };
    const { to } = replyAllRecipients(source, "me@alo.dev");
    expect(to.map((a) => a.email)).toEqual(["adam@gmail.com"]);
  });
});

describe("buildPrefill", () => {
  it("preserves every editable field from an existing draft", () => {
    const draft = {
      id: "draft-1",
      from: [addr("billing@alo.example")],
      to: [addr("accounts@customer.example")],
      cc: [addr("buyer@customer.example")],
      bcc: [addr("archive@alo.example")],
      subject: "Quote QUO-2026-00051",
      preview: "Fallback preview",
      inReplyTo: ["source-message"],
      references: ["source-thread"],
      textBody: [{ partId: "text", type: "text/plain" }],
      htmlBody: [{ partId: "html", type: "text/html" }],
      bodyValues: {
        text: { value: "Plain body", isTruncated: false },
        html: { value: "<p>Editable customer message</p>", isTruncated: false },
      },
      attachments: [],
    } as unknown as EmailFull;

    const prefill = buildPrefill({ mode: "edit", replyTo: draft }, "billing@alo.example");

    expect(prefill.to).toEqual(draft.to);
    expect(prefill.cc).toEqual(draft.cc);
    expect(prefill.bcc).toEqual(draft.bcc);
    expect(prefill.subject).toBe("Quote QUO-2026-00051");
    expect(prefill.body).toBe("<p>Editable customer message</p>");
    expect(prefill.inReplyTo).toEqual(["source-message"]);
    expect(prefill.references).toEqual(["source-thread"]);
  });
});

describe("materializeAttachments", () => {
  it("uploads existing message parts before reusing them in an edited draft", async () => {
    const downloadAttachment = vi.fn(async () => new Blob(["pdf"], { type: "application/pdf" }));
    const uploadFile = vi.fn(async () => ({
      blobId: "uploaded-pdf",
      type: "application/pdf",
      size: 3,
    }));

    const result = await materializeAttachments(
      { downloadAttachment, uploadFile },
      [
        {
          blobId: "message~part",
          name: "Quote.pdf",
          type: "application/pdf",
          size: 3,
          needsUpload: true,
        },
      ],
    );

    expect(downloadAttachment).toHaveBeenCalledWith("message~part", "Quote.pdf");
    expect(uploadFile).toHaveBeenCalledOnce();
    expect(result).toEqual([
      {
        blobId: "uploaded-pdf",
        name: "Quote.pdf",
        type: "application/pdf",
        size: 3,
        needsUpload: false,
      },
    ]);
  });
});
