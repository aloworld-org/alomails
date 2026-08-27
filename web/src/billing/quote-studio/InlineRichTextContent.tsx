import { sanitizeInlineRichText } from "./richText";

export function InlineRichTextContent({ value }: { value: string }) {
  return (
    <span
      className="[&_strong]:font-semibold"
      dangerouslySetInnerHTML={{ __html: sanitizeInlineRichText(value) }}
    />
  );
}
