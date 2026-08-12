// The one row of controls (ADR 0045).
//
// Eleven stylesheets declared `.toolbar` — the bar above a list, above a
// reading pane, above an editor. Four of them were byte-identical, and the
// differences among the rest came down to three things worth keeping: whether
// the row draws chrome of its own, how tight the gap is, and whether it lines
// its controls up on their centres or their baselines. Those are `surface`,
// `density` and `align`.
//
// What none of the eleven had:
//
//   * **A name, or a role.** Every one was a bare `<div>`. A screen a keyboard
//     user tabs into announces a run of unrelated buttons with nothing saying
//     they belong together — and `tasks` has two toolbars on one screen, which
//     from the outside are indistinguishable.
//   * **Wrapping.** Seven could not wrap, so at a narrow width their last
//     controls sat outside the pane, unreachable rather than merely unseen
//     (WCAG 1.4.10). That is why `ToolbarGroup` exists: once a row wraps, a
//     segmented cluster has to move as one item or it stops reading as one
//     control.
//   * **Arrow keys.** mail's formatting bar is a dozen icon buttons, which is
//     a dozen tab stops between the message list and the message body. A
//     toolbar of buttons should be one stop with arrow keys inside it.
//
// That last one is why `keyboard` is a choice rather than a default. The ARIA
// practice for `role="toolbar"` — one tab stop, arrows to move — assumes the
// toolbar holds buttons. Most of ours hold a search field and a select as
// well, and arrow keys inside a text field belong to the caret, not to us.
// So a mixed toolbar is announced as a named group and every control keeps its
// own tab stop, which is honest; `keyboard="roving"` opts into the toolbar
// role and the behaviour that role promises.
import {
  useEffect,
  useRef,
  type FocusEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";

import styles from "./Toolbar.module.css";

export interface ToolbarProps {
  /** What this row of controls acts on — "Invoice list", "Formatting".
   *  Required, and the thing all eleven copies left out: it is the group's
   *  accessible name, and the only way two toolbars on one screen are told
   *  apart by anything other than the eye. */
  label: string;
  /** The chrome the row draws. `plain` is nothing, for a toolbar inside a
   *  padded page. `bar` adds the padding and the rule under it that a header
   *  above a scrolling pane needs. `card` is a raised surface of its own. */
  surface?: "plain" | "bar" | "card" | undefined;
  /** `compact` closes the gap for a row of icon buttons, where the default
   *  spacing reads as separate things rather than one set of controls. */
  density?: "compact" | "default" | undefined;
  /** `end` lines controls up on their bottom edge, for a row of labelled
   *  fields: on centres, the labels drag the fields out of line. */
  align?: "center" | "end" | undefined;
  /** `roving` — the toolbar holds buttons and links only, so it becomes one
   *  tab stop with arrow keys, Home and End inside it (`role="toolbar"`).
   *  `tab` (default) — it also holds fields or selects, so it is a named group
   *  and each control keeps its own tab stop. */
  keyboard?: "roving" | "tab" | undefined;
  children: ReactNode;
  className?: string | undefined;
}

/** What roving focus moves between. Buttons and links only: `keyboard="roving"`
 *  is documented as being for a toolbar of buttons, and a text field caught in
 *  this list would lose its own arrow keys. */
const ITEMS = "button:not([disabled]), a[href]";

export function Toolbar({
  label,
  surface = "plain",
  density,
  align,
  keyboard = "tab",
  children,
  className,
}: ToolbarProps) {
  const root = useRef<HTMLDivElement>(null);
  // Which control is the tab stop. Held in a ref rather than state because
  // nothing renders from it, and kept across re-renders so that leaving the
  // toolbar and tabbing back returns to where you were rather than to item one.
  const active = useRef(0);
  const roving = keyboard === "roving";

  function items(): HTMLElement[] {
    const node = root.current;
    if (node === null) return [];
    // `offsetParent` is the natural visibility test and is always null under
    // jsdom, which would empty this list and silently disable the behaviour in
    // exactly the tests written to prove it. Attributes work in both.
    return [...node.querySelectorAll<HTMLElement>(ITEMS)].filter(
      (el) =>
        !el.hasAttribute("hidden") && el.getAttribute("aria-hidden") !== "true",
    );
  }

  // Apply the roving tab stop. Runs after every render because the controls in
  // a toolbar come and go — a button that appears when a row is selected has
  // to arrive with `tabindex="-1"` or the single tab stop quietly becomes two.
  useEffect(() => {
    if (!roving) return;
    const found = items();
    if (found.length === 0) return;
    if (active.current > found.length - 1) active.current = found.length - 1;
    found.forEach((el, i) => {
      el.tabIndex = i === active.current ? 0 : -1;
    });
  });

  function moveTo(found: HTMLElement[], index: number) {
    active.current = index;
    found.forEach((el, i) => {
      el.tabIndex = i === index ? 0 : -1;
    });
    found[index]?.focus();
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (!roving) return;
    const found = items();
    const from = found.indexOf(document.activeElement as HTMLElement);
    if (from === -1) return;
    // Right means "next" in the direction the text runs. No locale we ship is
    // right-to-left yet; when one arrives this is the line that has to know.
    const rtl =
      root.current !== null &&
      getComputedStyle(root.current).direction === "rtl";
    const forward = rtl ? "ArrowLeft" : "ArrowRight";
    const back = rtl ? "ArrowRight" : "ArrowLeft";
    let to: number;
    if (event.key === forward) to = (from + 1) % found.length;
    else if (event.key === back) to = (from - 1 + found.length) % found.length;
    else if (event.key === "Home") to = 0;
    else if (event.key === "End") to = found.length - 1;
    else return;
    // Home and End would otherwise scroll the pane under the toolbar, and the
    // arrows would scroll it sideways.
    event.preventDefault();
    moveTo(found, to);
  }

  function onFocus(event: FocusEvent<HTMLDivElement>) {
    if (!roving) return;
    // Clicking a control makes it the tab stop, so tabbing away and back
    // returns to the one last used rather than to the start of the row.
    const found = items();
    const index = found.indexOf(event.target);
    if (index === -1) return;
    active.current = index;
    found.forEach((el, i) => {
      el.tabIndex = i === index ? 0 : -1;
    });
  }

  const classes = [
    styles.toolbar,
    surface === "bar" ? styles.bar : surface === "card" ? styles.card : "",
    density === "compact" ? styles.compact : "",
    align === "end" ? styles.end : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div
      ref={root}
      className={classes}
      // `group` rather than `toolbar` unless the keyboard model is actually
      // there: announcing a toolbar tells a screen-reader user to expect arrow
      // keys, and a promise made in a role is still a promise.
      role={roving ? "toolbar" : "group"}
      aria-label={label}
      onKeyDown={onKeyDown}
      onFocus={onFocus}
    >
      {children}
    </div>
  );
}

export interface ToolbarGroupProps {
  /** Names the cluster — "Text style", "Alignment". Optional: a cluster that
   *  exists only to survive the wrap has nothing to say beyond the toolbar's
   *  own name, and an unnamed group is not announced at all, which is right. */
  label?: string | undefined;
  children: ReactNode;
  className?: string | undefined;
}

/** Controls that belong together. The toolbar wraps at a narrow width and this
 *  is what stops a wrap landing in the middle of a segmented control. */
export function ToolbarGroup({
  label,
  children,
  className,
}: ToolbarGroupProps) {
  const classes = [styles.group, className ?? ""].filter(Boolean).join(" ");
  return (
    <div
      className={classes}
      role={label === undefined ? undefined : "group"}
      aria-label={label}
    >
      {children}
    </div>
  );
}

/** Pushes what follows it to the far end of the row. Three stylesheets had
 *  this as `.toolbarSpacer`, all of them `flex: 1`. */
export function ToolbarSpacer() {
  return <div className={styles.spacer} aria-hidden="true" />;
}

/** A rule between two clusters of an editor toolbar. Decoration, so it is
 *  hidden: the grouping it draws is carried by `ToolbarGroup`. */
export function ToolbarDivider() {
  return <div className={styles.divider} aria-hidden="true" />;
}
