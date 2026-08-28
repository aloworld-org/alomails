// The list-style gallery: a word processor's "numbering library" / "bullet
// library" for a quotation list block.
//
// The trigger names the current style; the popover shows every style of the
// list's kind as a tile. Each tile is drawn by numbering a fixed five-item
// outline with the real catalogue, so what the tile shows is exactly what the
// document will show — there is no separate picture to keep in sync.
import { useCallback, useRef, useState } from "react";
import { ChevronDown } from "lucide-react";

import { cx, useDismiss } from "../../ds";
import { strings } from "../../i18n";
import { numberListItems, type ListItem } from "./listItems";
import {
  BULLET_STYLES,
  NUMBERING_STYLES,
  isNumberingStyle,
  type ListStyleId,
} from "./listStyles";

/** The outline every tile is drawn from: two levels of nesting under the
 *  first item, then a second top-level item — enough to show all three
 *  markers and how the top level continues. */
const SAMPLE: readonly ListItem[] = [
  { level: 0, text: "" },
  { level: 1, text: "" },
  { level: 1, text: "" },
  { level: 2, text: "" },
  { level: 0, text: "" },
];

function ListStyleTile({ style }: { style: ListStyleId }) {
  return (
    <span className="grid gap-1.5" aria-hidden="true">
      {numberListItems(SAMPLE, style).map((item, index) => (
        <span
          key={index}
          className={cx(
            "flex items-center gap-1.5",
            item.level === 1 && "pl-4",
            item.level === 2 && "pl-8",
          )}
        >
          <span className="w-9 shrink-0 text-right text-[11px] font-semibold leading-none text-primary">
            {item.marker}
          </span>
          <span className="h-1.5 flex-1 rounded-full bg-raised" />
        </span>
      ))}
    </span>
  );
}

export function ListStyleGallery({
  ordered,
  value,
  onChange,
}: {
  ordered: boolean;
  value: ListStyleId;
  onChange: (style: ListStyleId) => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, rootRef, close);

  const styles = ordered ? NUMBERING_STYLES : BULLET_STYLES;
  const groupLabel = ordered
    ? strings.quoteStudioNumberingStyle
    : strings.quoteStudioBulletStyle;

  return (
    <div className="relative" ref={rootRef}>
      <button
        type="button"
        className={cx(
          "flex h-control items-center gap-2 rounded-md !border !border-default bg-surface !px-3 text-sm text-primary transition-[border-color,box-shadow]",
          "hover:!border-accent/40 focus-visible:!border-accent focus-visible:outline-none focus-visible:shadow-[inset_0_0_0_1px_var(--accent)]",
          open && "!border-accent shadow-[inset_0_0_0_1px_var(--accent)]",
        )}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={groupLabel}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="text-xs font-semibold text-secondary">
          {strings.quoteStudioListStyle}
        </span>
        <span className="font-medium">{strings.quoteStudioListStyleName(value)}</span>
        <ChevronDown
          className={cx("size-4 shrink-0 text-tertiary transition-transform", open && "rotate-180")}
          aria-hidden="true"
        />
      </button>

      {open && (
        <div
          className="absolute left-0 top-full z-[var(--z-overlay)] mt-2 w-[min(23rem,calc(100vw-2rem))] rounded-xl border border-default bg-surface p-3 shadow-lg md:left-auto md:right-0"
          role="dialog"
          aria-label={groupLabel}
        >
          <div className="grid grid-cols-3 gap-2" role="radiogroup" aria-label={groupLabel}>
            {styles.map((style) => {
              const chosen = style === value;
              return (
                <button
                  key={style}
                  type="button"
                  role="radio"
                  aria-checked={chosen}
                  aria-label={strings.quoteStudioListStyleName(style)}
                  title={strings.quoteStudioListStyleName(style)}
                  className={cx(
                    // `!` because the global button reset strips borders.
                    "rounded-lg !border-2 bg-surface p-2.5 text-left transition-colors",
                    "hover:bg-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30",
                    chosen ? "!border-accent" : "!border-default",
                  )}
                  onClick={() => {
                    onChange(style);
                    setOpen(false);
                  }}
                >
                  <ListStyleTile style={style} />
                </button>
              );
            })}
          </div>
          {isNumberingStyle(value) === ordered ? null : (
            // Unreachable by construction (resolveListStyle keeps kind and
            // style in step); kept so a future mismatch fails visibly.
            <p className="mt-2 text-xs text-secondary">{strings.quoteStudioChooseListStyle}</p>
          )}
        </div>
      )}
    </div>
  );
}
