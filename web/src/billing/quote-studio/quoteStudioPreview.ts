import { generalTableHasContent } from "./generalTable";
import type { QuoteStudioBlock } from "./QuoteStudioBlock";

export function hasQuotePreviewText(value: string): boolean {
  return (
    value
      .replace(/<[^>]*>/g, "")
      .replaceAll("&nbsp;", " ")
      .trim().length > 0
  );
}

export function quoteBlockHasPreviewContent(block: QuoteStudioBlock): boolean {
  switch (block.kind) {
    case "pricing":
      return block.rowKeys === undefined || block.rowKeys.length > 0;
    case "table":
      return generalTableHasContent(block);
    case "heading":
    case "paragraph":
      return hasQuotePreviewText(block.text);
    case "quote":
      return (
        hasQuotePreviewText(block.text) ||
        hasQuotePreviewText(block.attribution)
      );
    case "list":
      return block.items.split("\n").some(hasQuotePreviewText);
    case "text":
      return (
        hasQuotePreviewText(block.heading) || hasQuotePreviewText(block.body)
      );
    case "image":
      return block.src.trim().length > 0;
    case "divider":
      return true;
  }
}
