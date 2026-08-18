// A dropdown menu: an icon-button trigger and a popover of actions. Closes on
// outside click, Escape, or selecting an item. Used for the reading-pane
// "Move to" and "More" menus; reusable anywhere a small action menu is needed.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The build that replaced this file's stylesheet changed no rule; two pieces
// of it had been leaning on source order and are now written down:
//
//   * **Placement is one string, not two.** `.up` reset `top` to `auto` and
//     set `bottom`, beating the base `top` by position alone. Here the popover
//     is pinned to one edge and offset from it by a margin, so up and down are
//     chosen between.
//   * **A danger item replaces the item's ink rather than layering over it.**
//     `.danger` and `.item` are one class each; the later one won.
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { ChevronDown } from "lucide-react";

import { IconButton } from "./IconButton";
import { cx } from "./cx";
import { useDismiss } from "./useDismiss";

/** A labelled text-button trigger (e.g. a "New ▾" menu). `py-[7px]` is this
 *  control's own height, written as the stylesheet wrote it: it is a third
 *  button height beside `--button-height-sm` and `-md`, which is flagged for
 *  the D1.55 wave check rather than quietly rounded here. */
const TEXT_TRIGGER =
  "inline-flex items-center gap-1.5 py-[7px] px-3 rounded-md " +
  "border border-default bg-surface text-primary text-sm font-medium " +
  "whitespace-nowrap hover:bg-raised hover:border-strong";

/** Open looks like hover, which is what the stylesheet said — the same two
 *  values, so the two may safely be on the element at once. */
const TEXT_TRIGGER_OPEN = "bg-raised border-strong";

/** The popover. `max-w-[calc(100vw-var(--space-4))]` keeps a wide menu inside
 *  a phone-width window; `z-[var(--z-overlay)]` is the layer tokens.css puts
 *  transient surfaces on. */
const POPOVER =
  "absolute min-w-50 max-h-80 overflow-y-auto p-2 " +
  "bg-surface border border-subtle rounded-lg shadow-lg " +
  "z-[var(--z-overlay)] max-w-[calc(100vw-var(--space-4))]";

/** Which edge it hangs from, and which edge it lines up with. Pinned with a
 *  margin rather than a `calc`, so up and down are one exclusive choice. */
const PLACE = { down: "top-full mt-1", up: "bottom-full mb-1" } as const;
const EDGE = { start: "left-0", end: "right-0" } as const;

/** An action. The ink is chosen once, below, because a danger item replaces it
 *  rather than tinting over it. */
const ITEM =
  "flex items-center gap-3 w-full py-2 px-3 rounded-md " +
  "text-base text-left whitespace-nowrap " +
  "transition-colors duration-[var(--duration-fast)] ease-standard " +
  "enabled:hover:bg-raised disabled:opacity-45 disabled:cursor-not-allowed";

export interface MenuItem {
  key: string;
  label: string;
  icon?: ReactNode;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
  /** Draw a separator line above this item (groups the menu). */
  divider?: boolean;
}

interface MenuProps {
  /** Accessible name for the trigger. */
  label: string;
  /** Trigger icon. */
  icon: ReactNode;
  items: MenuItem[];
  /** Which edge the popover aligns to (default: end/right). */
  align?: "start" | "end";
  /** When set, the trigger is a labelled text button (e.g. "New ▾") instead of
   *  an icon-only button. */
  triggerLabel?: string;
}

export function Menu({
  label,
  icon,
  items,
  align = "end",
  triggerLabel,
}: MenuProps) {
  const [open, setOpen] = useState(false);
  const [placement, setPlacement] = useState<{
    align: "start" | "end";
    up: boolean;
  }>({ align, up: false });
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    if (!open || ref.current === null) return;
    const rect = ref.current.getBoundingClientRect();
    const menuWidth = 216;
    const resolvedAlign =
      align === "end" && rect.right < menuWidth + 8
        ? "start"
        : align === "start" && window.innerWidth - rect.left < menuWidth + 8
          ? "end"
          : align;
    setPlacement({
      align: resolvedAlign,
      up: window.innerHeight - rect.bottom < 340 && rect.top > 340,
    });
  }, [align, open]);

  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, ref, close);

  return (
    <div className="relative inline-flex" ref={ref}>
      {triggerLabel !== undefined ? (
        <button
          type="button"
          className={cx(TEXT_TRIGGER, open && TEXT_TRIGGER_OPEN)}
          aria-label={label}
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          {icon}
          <span>{triggerLabel}</span>
          <ChevronDown size={15} className="text-tertiary" aria-hidden />
        </button>
      ) : (
        <IconButton
          size="sm"
          label={label}
          icon={icon}
          active={open}
          onClick={() => setOpen((v) => !v)}
        />
      )}
      {open && (
        <div
          className={cx(
            POPOVER,
            EDGE[placement.align],
            PLACE[placement.up ? "up" : "down"],
          )}
          role="menu"
        >
          {items.map((item) => (
            // `contents`, so the optional divider and the button are siblings
            // in the menu's own box rather than a row of their own.
            <div key={item.key} className="contents">
              {item.divider === true && (
                <div className="h-px my-1 mx-2 bg-subtle" role="separator" />
              )}
              <button
                type="button"
                role="menuitem"
                className={cx(
                  ITEM,
                  item.danger === true ? "text-danger" : "text-primary",
                )}
                disabled={item.disabled}
                onClick={() => {
                  setOpen(false);
                  item.onClick();
                }}
              >
                {item.icon !== undefined && (
                  <span
                    className={cx(
                      "inline-flex shrink-0 [&_svg]:size-4",
                      item.danger === true ? "text-danger" : "text-secondary",
                    )}
                  >
                    {item.icon}
                  </span>
                )}
                <span className="flex-1 overflow-hidden text-ellipsis">
                  {item.label}
                </span>
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
