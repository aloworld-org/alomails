import { ArrowDown, ArrowUp, Plus, Trash2 } from "lucide-react";
import { cx } from "../../ds";
import { strings } from "../../i18n";
import { BlockCommand } from "./BlockCommand";
import { InlineRichTextEditor } from "./InlineRichTextEditor";
import { TextColumnsPicker } from "./TextColumnsPicker";

export function ListBlockEditor({
  ordered,
  items,
  columns,
  onChange,
}: {
  ordered: boolean;
  items: string;
  columns: 1 | 2 | 3;
  onChange: (patch: { items?: string; columns?: 1 | 2 | 3 }) => void;
}) {
  const rows = items === "" ? [""] : items.split("\n");
  const replace = (index: number, value: string) =>
    onChange({
      items: rows
        .map((item, itemIndex) => (itemIndex === index ? value : item))
        .join("\n"),
    });
  const remove = (index: number) => {
    const next = rows.filter((_, itemIndex) => itemIndex !== index);
    onChange({ items: next.length === 0 ? "" : next.join("\n") });
  };
  const move = (index: number, direction: -1 | 1) => {
    const destination = index + direction;
    if (destination < 0 || destination >= rows.length) return;
    const next = [...rows];
    const [item] = next.splice(index, 1);
    if (item === undefined) return;
    next.splice(destination, 0, item);
    onChange({ items: next.join("\n") });
  };

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="text-sm font-semibold text-primary">
            {strings.quoteStudioListLayout}
          </p>
          <p className="mt-0.5 text-xs text-secondary">
            {strings.quoteStudioListLayoutHelp}
          </p>
        </div>
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
      <div
        className={cx(
          "grid gap-2",
          columns === 2 && "md:grid-cols-2",
          columns === 3 && "md:grid-cols-2 xl:grid-cols-3",
        )}
      >
        {rows.map((item, index) => (
          <div
            key={index}
            className="group/list-item grid grid-cols-[2.25rem_minmax(0,1fr)_7.75rem] items-center gap-3 rounded-xl border border-default bg-surface p-3 shadow-sm transition-colors hover:border-accent/30 focus-within:border-accent/30 max-md:grid-cols-[2.25rem_minmax(0,1fr)]"
          >
            <span className="grid size-9 place-items-center rounded-lg bg-raised text-sm font-semibold text-secondary">
              {ordered ? index + 1 : "•"}
            </span>
            <InlineRichTextEditor
              value={item}
              aria-label={
                ordered
                  ? strings.quoteStudioNumberedItemA11y(index + 1)
                  : strings.quoteStudioBulletItemA11y(index + 1)
              }
              placeholder={strings.quoteStudioWriteItem}
              onChange={(value) => replace(index, value)}
            />
            <div className="flex items-center justify-end gap-1 opacity-0 transition-opacity group-hover/list-item:opacity-100 group-focus-within/list-item:opacity-100 max-md:col-span-2 max-md:justify-self-end max-md:opacity-100">
              <BlockCommand
                label={strings.quoteStudioMoveItemUp}
                disabled={index === 0}
                onClick={() => move(index, -1)}
              >
                <ArrowUp className="size-4" />
              </BlockCommand>
              <BlockCommand
                label={strings.quoteStudioMoveItemDown}
                disabled={index === rows.length - 1}
                onClick={() => move(index, 1)}
              >
                <ArrowDown className="size-4" />
              </BlockCommand>
              <BlockCommand
                label={strings.quoteStudioRemoveItem}
                danger
                onClick={() => remove(index)}
              >
                <Trash2 className="size-4" />
              </BlockCommand>
            </div>
          </div>
        ))}
      </div>
      <button
        type="button"
        className="mt-3 inline-flex min-h-10 items-center gap-2 rounded-lg border border-default bg-surface px-4 text-sm font-semibold text-primary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent"
        onClick={() => onChange({ items: items === "" ? "\n" : `${items}\n` })}
      >
        <Plus className="size-4" aria-hidden="true" />{" "}
        {strings.quoteStudioAddItemBelow}
      </button>
    </div>
  );
}
