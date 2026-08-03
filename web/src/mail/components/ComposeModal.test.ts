import { describe, expect, it } from "vitest";

import { replyAllRecipients } from "./ComposeModal";

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
