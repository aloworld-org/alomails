import { useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  IndentDecrease,
  IndentIncrease,
  Plus,
  Trash2,
} from "lucide-react";
import { cx } from "../../ds";
import { strings } from "../../i18n";
import { BlockCommand } from "./BlockCommand";
import { InlineRichTextEditor } from "./InlineRichTextEditor";
import { ListStyleGallery } from "./ListStyleGallery";
import {
  canIndentListItem,
  numberListItems,
  parseListItems,
  serializeListItems,
  shiftListItemLevel,
  type ListItem,
} from "./listItems";
import type { ListStyleId } from "./listStyles";
import { TextColumnsPicker } from "./TextColumnsPicker";

export function ListBlockEditor({
  ordered,
  items,
  columns,
  style,
  onChange,
}: {
  ordered: boolean;
  items: string;
  columns: 1 | 2 | 3;
  style: ListStyleId;
  onChange: (patch: {
    items?: string;
    columns?: 1 | 2 | 3;
    style?: ListStyleId;
  }) => void;
}) {
  const [activeIndex, setActiveIndex] = useState(0);
  const parsed = parseListItems(items);
  // An empty list still shows one row to type into.
  const rows: ListItem[] = parsed.length === 0 ? [{ level: 0, text: "" }] : parsed;
  const numbered = numberListItems(rows, style);
  const selectedIndex = Math.min(activeIndex, rows.length - 1);
  const selectedItem = rows[selectedIndex];
  const commit = (next: ListItem[]) =>
    onChange({ items: next.length === 0 ? "" : serializeListItems(next) });
  const replace = (index: number, text: string) =>
    commit(rows.map((item, itemIndex) => (itemIndex === index ? { ...item, text } : item)));
  const remove = (index: number) =>
    commit(rows.filter((_, itemIndex) => itemIndex !== index));
  const move = (index: number, direction: -1 | 1) => {
    const destination = index + direction;
    if (destination < 0 || destination >= rows.length) return;
    const next = [...rows];
    const [item] = next.splice(index, 1);
    if (item === undefined) return;
    next.splice(destination, 0, item);
    commit(next);
  };
  const shift = (index: number, step: -1 | 1) =>
    commit(shiftListItemLevel(rows, index, step));
  // A new item continues at the level of the one above it, as it would when
  // pressing Enter at the end of a nested item.
  const addBelow = () =>
    commit([...rows, { level: rows[rows.length - 1]?.level ?? 0, text: "" }]);

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="text-sm font-semibold text-primary">
            {strings.quoteStudioListLayout}
          </p>
          <div className="mt-1 flex flex-wrap items-center gap-2">
            <p className="text-xs text-secondary">
              {strings.quoteStudioListLayoutHelp}
            </p>
            <div
              role="toolbar"
              aria-label={strings.quoteStudioListItemFormatting}
              className="flex items-center gap-1"
            >
              <BlockCommand
                label={strings.quoteStudioOutdentItem}
                disabled={selectedItem?.level === 0}
                onClick={() => shift(selectedIndex, -1)}
              >
                <IndentDecrease className="size-4" />
              </BlockCommand>
              <BlockCommand
                label={strings.quoteStudioIndentItem}
                disabled={!canIndentListItem(rows, selectedIndex)}
                onClick={() => shift(selectedIndex, 1)}
              >
                <IndentIncrease className="size-4" />
              </BlockCommand>
              <BlockCommand
                label={strings.quoteStudioMoveItemUp}
                disabled={selectedIndex === 0}
                onClick={() => move(selectedIndex, -1)}
              >
                <ArrowUp className="size-4" />
              </BlockCommand>
              <BlockCommand
                label={strings.quoteStudioMoveItemDown}
                disabled={selectedIndex === rows.length - 1}
                onClick={() => move(selectedIndex, 1)}
              >
                <ArrowDown className="size-4" />
              </BlockCommand>
              <BlockCommand
                label={strings.quoteStudioRemoveItem}
                danger
                onClick={() => remove(selectedIndex)}
              >
                <Trash2 className="size-4" />
              </BlockCommand>
            </div>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <ListStyleGallery
            ordered={ordered}
            value={style}
            onChange={(next) => onChange({ style: next })}
          />
          <TextColumnsPicker
            value={columns}
            label={
              ordered
                ? strings.quoteStudioNumberedListColumns
                : strings.quoteStudioBulletListColumns
            }
            onChange={(next) => onChange({ columns: next })}
          />
        </div>
      </div>
      <div
        className={cx(
          "grid gap-2",
          columns === 2 && "md:grid-cols-2",
          columns === 3 && "md:grid-cols-2 xl:grid-cols-3",
        )}
      >
        {numbered.map((item, index) => (
          <div
            key={index}
            onFocusCapture={() => setActiveIndex(index)}
            onClick={() => setActiveIndex(index)}
            className={cx(
              "group/list-item rounded-xl border border-default bg-surface p-3 shadow-sm transition-colors hover:border-accent/30 focus-within:border-accent/30",
              selectedIndex === index && "border-accent/35",
              item.level === 1 && "ml-6",
              item.level === 2 && "ml-12",
            )}
          >
            <div className="mb-2 flex min-h-9 items-center gap-3">
              <span
                className="grid min-h-9 min-w-9 place-items-center rounded-lg bg-raised px-1.5 text-xs font-semibold tabular-nums text-secondary"
                aria-hidden="true"
              >
                {item.marker}
              </span>
            </div>
            <InlineRichTextEditor
              value={item.text}
              aria-label={
                ordered
                  ? strings.quoteStudioNumberedItemA11y(index + 1)
                  : strings.quoteStudioBulletItemA11y(index + 1)
              }
              placeholder={strings.quoteStudioWriteItem}
              onChange={(value) => replace(index, value)}
            />
          </div>
        ))}
      </div>
      <button
        type="button"
        className="mt-3 inline-flex min-h-10 items-center gap-2 rounded-lg border border-default bg-surface px-4 text-sm font-semibold text-primary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
        onClick={addBelow}
      >
        <Plus className="size-4" aria-hidden="true" />{" "}
        {strings.quoteStudioAddItemBelow}
      </button>
    </div>
  );
}
