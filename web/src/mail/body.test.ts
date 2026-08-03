import { describe, expect, it } from "vitest";

import { splitQuotedHtml, splitQuotedText } from "./body";

describe("splitQuotedText", () => {
  it("splits at an 'On … wrote:' attribution", () => {
    const text = "Thanks, that works!\n\nOn Tue, Jul 29, Alice wrote:\n> original\n> more";
    const { main, quoted } = splitQuotedText(text);
    expect(main).toBe("Thanks, that works!");
    expect(quoted).toContain("On Tue, Jul 29, Alice wrote:");
    expect(quoted).toContain("> original");
  });

  it("splits at a run of quoted '>' lines", () => {
    const text = "My reply here.\n> quoted line 1\n> quoted line 2";
    const { main, quoted } = splitQuotedText(text);
    expect(main).toBe("My reply here.");
    expect(quoted).toBe("> quoted line 1\n> quoted line 2");
  });

  it("splits at an Original Message separator", () => {
    const text = "See below.\n\n----- Original Message -----\nFrom: bob";
    const { quoted } = splitQuotedText(text);
    expect(quoted).toContain("Original Message");
  });

  it("returns no quote when there is none", () => {
    const { main, quoted } = splitQuotedText("Just a plain message, nothing quoted.");
    expect(main).toBe("Just a plain message, nothing quoted.");
    expect(quoted).toBeNull();
  });
});

describe("splitQuotedHtml", () => {
  it("splits at a blockquote", () => {
    const html = "<p>new reply</p><blockquote>old thread</blockquote>";
    const { main, quoted } = splitQuotedHtml(html);
    expect(main).toBe("<p>new reply</p>");
    expect(quoted).toBe("<blockquote>old thread</blockquote>");
  });

  it("splits at a gmail_quote container", () => {
    const html = '<div>hi</div><div class="gmail_quote">old</div>';
    const { main, quoted } = splitQuotedHtml(html);
    expect(main).toBe("<div>hi</div>");
    expect(quoted).toContain("gmail_quote");
  });

  it("returns no quote when there is none", () => {
    const { quoted } = splitQuotedHtml("<p>just a note</p>");
    expect(quoted).toBeNull();
  });
});
