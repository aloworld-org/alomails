import { strings } from "../../i18n";
import type { QuoteStudioBlock } from "./QuoteStudioBlock";

export function quoteStudioBlockName(block: QuoteStudioBlock): string {
  switch (block.kind) {
    case "heading":
      return strings.quoteStudioHeading;
    case "paragraph":
      return strings.quoteStudioParagraph;
    case "quote":
      return strings.quoteStudioQuote;
    case "list":
      return block.ordered
        ? strings.quoteStudioNumberedList
        : strings.quoteStudioBulletList;
    case "divider":
      return strings.quoteStudioDivider;
    case "image":
      return strings.quoteStudioImage;
    case "pricing":
      return strings.quoteStudioPricingTable;
    case "table":
      return strings.quoteStudioTable;
    default:
      return strings.quoteStudioCategoryText;
  }
}
