// The read-only rendering of a list block — what the customer sees.
//
// Items are numbered with the block's style and indented by their level;
// empty lines are skipped, as they always were. Markers are plain text in the
// document's marker colour so every scheme fits — a "1.2.1." or a "VIII." has
// no place in a fixed-size badge.
import { cx } from "../../ds";
import { InlineRichTextContent } from "./InlineRichTextContent";
import { numberListItems, parseListItems } from "./listItems";
import { resolveListStyle } from "./listStyles";
import type { ListBlock } from "./QuoteStudioBlock";

export function ListBlockContent({ block }: { block: ListBlock }) {
  const style = resolveListStyle(block.style, block.ordered);
  const items = numberListItems(
    parseListItems(block.items).filter((item) => item.text !== ""),
    style,
  );
  const columns = block.columns ?? 1;
  const Tag = block.ordered ? "ol" : "ul";
  return (
    <Tag
      className={cx(
        "grid list-none gap-x-10 gap-y-3",
        columns === 2 && "md:grid-cols-2",
        columns === 3 && "md:grid-cols-3",
      )}
    >
      {items.map((item, index) => (
        <li
          key={index}
          className={cx(
            "flex min-w-0 items-start gap-3",
            item.level === 1 && "pl-6",
            item.level === 2 && "pl-12",
          )}
        >
          <span
            className={cx(
              "shrink-0 pt-0.5 font-semibold tabular-nums leading-6",
              block.ordered
                ? "min-w-6 text-sm text-[var(--quote-number-marker)]"
                : "text-xs text-[var(--quote-bullet-marker)]",
            )}
            aria-hidden="true"
          >
            {item.marker}
          </span>
          <span className="min-w-0 flex-1 pt-0.5">
            <InlineRichTextContent value={item.text} />
          </span>
        </li>
      ))}
    </Tag>
  );
}
