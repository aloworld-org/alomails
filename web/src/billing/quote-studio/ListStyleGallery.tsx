// The list-style gallery: a word processor's "numbering library" / "bullet
// library" for a quotation list block.
//
// The trigger names the current style; the popover shows every style of the
// list's kind as a tile. Each tile is drawn by numbering a fixed five-item
// outline with the real catalogue, so what the tile shows is exactly what the
// document will show — there is no separate picture to keep in sync.
import { useCallback, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";

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

function CompactListStylePreview({ style }: { style: ListStyleId }) {
  return (
    <span className="grid w-16 gap-1" aria-hidden="true">
      {numberListItems(SAMPLE.slice(0, 3), style).map((item, index) => (
        <span key={index} className="flex items-center gap-1.5">
          <span
            className="w-5 shrink-0 text-right text-[9px] font-semibold leading-none text-primary before:content-[attr(data-marker)]"
            data-marker={item.marker}
          />
          <span className="h-1 flex-1 rounded-full bg-raised" />
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
          "flex h-control items-center gap-2 rounded-md !border !border-default bg-surface !px-2.5 text-primary transition-[border-color,box-shadow]",
          "hover:!border-accent/40 focus-visible:!border-accent focus-visible:outline-none focus-visible:shadow-[inset_0_0_0_1px_var(--accent)]",
          open && "!border-accent shadow-[inset_0_0_0_1px_var(--accent)]",
        )}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={groupLabel}
        onClick={() => setOpen((current) => !current)}
      >
        <CompactListStylePreview style={value} />
        <ChevronDown
          className={cx("size-4 shrink-0 text-tertiary transition-transform", open && "rotate-180")}
          aria-hidden="true"
        />
      </button>

      {open && (
        <>
          <div
            className="fixed inset-0 z-[calc(var(--z-overlay)-1)] bg-overlay sm:hidden"
            role="presentation"
            onMouseDown={close}
          />
          <div
            className="fixed inset-x-4 bottom-4 z-[var(--z-overlay)] mx-auto max-h-[calc(100vh-2rem)] w-auto max-w-[23rem] overflow-y-auto overscroll-contain rounded-2xl border border-default bg-surface p-4 shadow-xl sm:absolute sm:inset-x-auto sm:bottom-auto sm:right-0 sm:top-full sm:mx-0 sm:mt-2 sm:w-[23rem]"
            role="dialog"
            aria-label={groupLabel}
          >
            <span
              className="mx-auto mb-3 block h-1 w-10 rounded-full bg-raised sm:hidden"
              aria-hidden="true"
            />
            <div
              className="grid grid-cols-2 gap-2 sm:grid-cols-3"
              role="radiogroup"
              aria-label={groupLabel}
            >
              {styles.map((style) => {
                const chosen = style === value;
                const styleName = strings.quoteStudioListStyleName(style);
                return (
                  <button
                    key={style}
                    type="button"
                    role="radio"
                    aria-checked={chosen}
                    aria-label={styleName}
                    className={cx(
                      // `!` because the global button reset strips borders.
                      "relative min-h-24 min-w-0 rounded-xl !border-2 p-3 text-left transition-[background-color,border-color,box-shadow]",
                      "hover:!border-accent/50 hover:bg-accent-soft/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30",
                      chosen
                        ? "!border-accent bg-accent-soft/60 shadow-sm"
                        : "!border-default bg-surface",
                    )}
                    onClick={() => {
                      onChange(style);
                      setOpen(false);
                    }}
                  >
                    <ListStyleTile style={style} />
                    {chosen && (
                      <span className="absolute right-1.5 top-1.5 flex size-5 items-center justify-center rounded-full bg-accent text-on-accent shadow-sm">
                        <Check className="size-3" aria-hidden="true" />
                      </span>
                    )}
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
        </>
      )}
    </div>
  );
}
