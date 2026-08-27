const INLINE_TAGS = new Set(["B", "EM", "I", "STRONG"]);
const RICH_TEXT_TAGS = new Set([
  "B",
  "BR",
  "EM",
  "H1",
  "H2",
  "H3",
  "I",
  "LI",
  "OL",
  "P",
  "STRONG",
  "UL",
]);

function sanitize(value: string, allowedTags: Set<string>): string {
  const template = document.createElement("template");
  template.innerHTML = value;
  for (const element of [...template.content.querySelectorAll("*")]) {
    if (!allowedTags.has(element.tagName)) {
      element.replaceWith(...element.childNodes);
      continue;
    }
    for (const attribute of [...element.attributes]) {
      element.removeAttribute(attribute.name);
    }
  }
  return template.innerHTML;
}

export function sanitizeInlineRichText(value: string): string {
  return sanitize(value, INLINE_TAGS);
}

export function sanitizeRichText(value: string): string {
  const hadMarkup = value.includes("<");
  const sanitized = sanitize(value, RICH_TEXT_TAGS);
  return hadMarkup ? sanitized : sanitized.replaceAll("\n", "<br>");
}
