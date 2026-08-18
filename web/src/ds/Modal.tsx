// The one modal (ADR 0045).
//
// Sixteen stylesheets declared their own `.modal` before this existed, and the
// visual differences were the least of it. A search of the codebase found **no
// focus trap anywhere**, and only two modules handling Escape — so most of
// those dialogs could not be closed from a keyboard, and Tab walked straight
// out of them onto the page behind, where a screen reader would happily read
// out a form the user could no longer see.
//
// That is the argument for components over convention in one paragraph. Nobody
// was careless; the behaviour is simply invisible when you are looking at a
// design, and it has to be written once and then inherited.
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The stylesheet this replaces resolved its variants by source order —
// `.page .body` after `.body` — which Tailwind cannot do: two utilities setting
// one property have no defined winner, because they are emitted in Tailwind's
// order rather than in the order they appear in `class`. So the panel's height
// and the body's padding, gap and overflow are each chosen once, as whole
// mutually exclusive strings, rather than layered and hoped over.
import { useEffect, useRef, type ReactNode } from "react";

export interface ModalProps {
  /** Names the dialog for assistive technology, and renders as its heading. */
  title: string;
  onClose: () => void;
  /** A glyph before the title. Decoration — the title is the name, so this is
   *  hidden from assistive technology. Added for the two authoring dialogs,
   *  which each open with an accent-coloured mark (Σ for an equation, `</>`
   *  for a code block) that says which editor you are in before the words do. */
  icon?: ReactNode | undefined;
  /** Header controls — a close button, usually. */
  actions?: ReactNode | undefined;
  footer?: ReactNode | undefined;
  /** 720px instead of 480px, for a dialog that carries a table or two columns. */
  wide?: boolean | undefined;
  /** A dialog with a browser inside it — a symbol palette, a language list.
   *  Its content changes size with every keystroke, and a dialog that resizes
   *  under the pointer while you type is unusable, so the panel takes a fixed
   *  height and hands the scrolling to whichever child of the body asks for it
   *  (`flex: 1; min-height: 0`). Without this the body scrolls as one piece and
   *  the thing you are editing scrolls away from the thing you are picking.
   *
   *  `"page"` is the same argument for a dialog that is a place rather than a
   *  question — Settings, which has its own section navigation. It takes the
   *  height of the window rather than of its content, so moving from General
   *  to Filters does not resize the panel under the pointer, and its body is
   *  flush: a two-pane layout draws its own edges, and padding here would push
   *  the nav column off the panel's own side. Added for `shell/SettingsModal`
   *  (D2.03), which was the only `.modal` of the sixteen that was a place. */
  tall?: boolean | "page" | undefined;
  children: ReactNode;
}

/** Everything that can hold focus inside the panel, in tab order. */
const FOCUSABLE =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** The focusable controls of the panel, in tab order, minus the ones that are
 *  not on the page. A `hidden` control matches the selector above and cannot
 *  take focus, so it has to be filtered in both places that walk the list —
 *  the trap always did, and the opening focus did not, which meant a dialog
 *  whose first element was a hidden file input (`contacts`, D2.06: the picker
 *  the Import button clicks) opened with focus still on the page behind it.
 *  `focus()` on a hidden element is a silent no-op, so nothing said so.
 *
 *  `offsetParent` would be the natural visibility test and is always null in
 *  jsdom, which would silently empty this list and disable the trap under
 *  test. Attributes work in both. */
function focusableIn(node: HTMLElement): HTMLElement[] {
  return [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) =>
      !el.hasAttribute("hidden") && el.getAttribute("aria-hidden") !== "true",
  );
}

/** The scrim. It fills the viewport and centres the panel; the padding is what
 *  keeps a full-height dialog off the edges of the window, and what
 *  `--modal-max-height` subtracts. */
const OVERLAY =
  "fixed inset-0 z-[var(--z-modal)] flex items-center justify-center p-6 bg-overlay";

/** The panel. `overflow-hidden` is what makes the rounded corners clip the
 *  header's border and the body's scrollbar. */
const PANEL =
  "flex flex-col w-full max-h-[var(--modal-max-height)] rounded-xl bg-surface shadow-lg overflow-hidden";

/** The panel's height, chosen once. Default is the content's own height, up to
 *  the window; `tall` and `page` fix it for the reasons on the prop below. */
const HEIGHT = {
  auto: "",
  tall: "h-[var(--modal-height-tall)]",
  page: "h-[var(--modal-height-page)]",
} as const;

/** The body, chosen once for the same reason. It is the body that scrolls, not
 *  the overlay: a long form must not push its own footer off the screen, which
 *  is what several of the sixteen copies did. Under `tall` the panel's height
 *  stops moving and the body stops scrolling as one piece, so whichever child
 *  asks for the room (`flex-1 min-h-0`) is the thing that scrolls. Under
 *  `page` the body is flush as well — a two-pane layout draws its own edges
 *  and its own padding, and padding here would push the nav column off the
 *  panel's side. */
const BODY = {
  auto: "flex flex-col gap-4 p-5 overflow-y-auto",
  tall: "flex flex-col gap-4 p-5 overflow-hidden",
  page: "flex flex-col gap-0 p-0 overflow-hidden",
} as const;

const HEAD = "flex items-center gap-2 px-5 py-4 border-b border-subtle";
const TITLE = "flex-1 m-0 text-lg font-semibold text-primary";
/** The mark before the title. Decoration: the title is the name. */
const ICON = "inline-flex items-center text-accent";
const FOOT = "flex items-center gap-3 px-5 py-4 border-t border-subtle";

export function Modal({
  title,
  onClose,
  icon,
  actions,
  footer,
  wide,
  tall,
  children,
}: ModalProps) {
  const panel = useRef<HTMLDivElement>(null);
  const shape = tall === true ? "tall" : tall === "page" ? "page" : "auto";

  useEffect(() => {
    // Where focus was, so it can be given back. Without this, dismissing a
    // dialog drops the caret at the top of the document and a keyboard user
    // has to travel back to whatever they were doing.
    const opener = document.activeElement as HTMLElement | null;
    const node = panel.current;
    // Not `a?.focus() ?? b.focus()`: `focus()` returns undefined, so the
    // fallback ran every time and pulled focus straight back to the panel.
    const firstControl = node === null ? undefined : focusableIn(node)[0];
    if (firstControl) firstControl.focus();
    else node?.focus();

    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "Tab" || node === null) return;
      // The trap. Tab from the last control returns to the first, and
      // Shift+Tab from the first goes to the last, so focus cannot leave a
      // dialog that is covering the page.
      const focusable = focusableIn(node);
      if (focusable.length === 0) return;
      const first = focusable[0]!;
      const last = focusable[focusable.length - 1]!;
      const active = document.activeElement;
      if (event.shiftKey && active === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    }

    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      opener?.focus?.();
    };
  }, [onClose]);

  return (
    <div
      className={OVERLAY}
      // A click on the backdrop dismisses; a click that started inside the
      // panel and ended on the backdrop — a drag while selecting text — does
      // not, which is why this tests the target rather than using a bubbled
      // click from the panel.
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={panel}
        className={[
          PANEL,
          wide === true
            ? "max-w-[var(--modal-width-wide)]"
            : "max-w-[var(--modal-width)]",
          HEIGHT[shape],
        ]
          .filter(Boolean)
          .join(" ")}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
      >
        <div className={HEAD}>
          {icon !== undefined && (
            <span className={ICON} aria-hidden="true">
              {icon}
            </span>
          )}
          <h2 className={TITLE}>{title}</h2>
          {actions}
        </div>
        <div className={BODY[shape]}>{children}</div>
        {footer !== undefined && <div className={FOOT}>{footer}</div>}
      </div>
    </div>
  );
}
