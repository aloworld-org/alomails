import { describe, expect, it } from "vitest";
import { sanitizeInlineRichText, sanitizeRichText } from "./richText";

describe("quote studio rich text sanitizers", () => {
  it("keeps only supported inline formatting", () => {
    expect(sanitizeInlineRichText('<strong class="x">Safe</strong><a href="#"> link</a>')).toBe(
      "<strong>Safe</strong> link",
    );
  });

  it("keeps supported document structure and removes attributes", () => {
    expect(sanitizeRichText('<h2 style="color:red">Title</h2><script>bad</script>')).toBe(
      "<h2>Title</h2>bad",
    );
  });
});
