// The one "Columns" control for content blocks in the quotation studio.
//
// Lists had it first; paragraphs and quotes now carry the same setting, and a
// setting that means the same thing in three places is one component, not
// three copies of a ChoicePicker that drift apart. The number semantics live
// here too — a saved design from any version, or one hand-edited in storage,
// reads as a valid column count rather than as `undefined` in a class name.
import { ChoicePicker } from "../../ds";
import { strings } from "../../i18n";

export type TextColumns = 1 | 2 | 3;

/** Any saved value → a valid column count. Missing or out of range is one
 *  column, which is exactly what every block had before the setting existed. */
export function textColumns(value: unknown): TextColumns {
  return value === 2 || value === 3 ? value : 1;
}

/** The layout classes for prose flowed into `columns` — none for one column,
 *  so a single-column block renders exactly as it did before. Newspaper
 *  columns rather than a grid, because prose has no natural cells: the text
 *  fills the first column and continues into the next. `break-inside-avoid`
 *  keeps a paragraph or list item from being cut between columns. Below `md`
 *  the columns collapse to one — two columns of text on a phone is two
 *  unreadable strips. */
export function textColumnsClass(columns: TextColumns): string {
  if (columns === 2) return "md:columns-2 md:gap-x-10 [&>*]:break-inside-avoid";
  if (columns === 3) return "md:columns-3 md:gap-x-10 [&>*]:break-inside-avoid";
  return "";
}

export function TextColumnsPicker({
  value,
  label,
  onChange,
}: {
  value: TextColumns;
  /** Names the control for assistive technology — which block's columns. */
  label: string;
  onChange: (columns: TextColumns) => void;
}) {
  return (
    <div className="flex items-center gap-2 text-xs font-semibold text-secondary">
      <span>{strings.quoteStudioColumns}</span>
      <div className="w-36">
        <ChoicePicker
          value={String(value)}
          label={label}
          placeholder={strings.quoteStudioChooseColumns}
          options={[
            { value: "1", label: strings.quoteStudioColumnCount(1) },
            { value: "2", label: strings.quoteStudioColumnCount(2) },
            { value: "3", label: strings.quoteStudioColumnCount(3) },
          ]}
          onChange={(next) => onChange(textColumns(Number(next)))}
        />
      </div>
    </div>
  );
}
