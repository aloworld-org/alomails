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
import { useEffect, useRef, type ReactNode } from "react";

import styles from "./Modal.module.css";

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
   *  the thing you are editing scrolls away from the thing you are picking. */
  tall?: boolean | undefined;
  children: ReactNode;
}

/** Everything that can hold focus inside the panel, in tab order. */
const FOCUSABLE =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

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

  useEffect(() => {
    // Where focus was, so it can be given back. Without this, dismissing a
    // dialog drops the caret at the top of the document and a keyboard user
    // has to travel back to whatever they were doing.
    const opener = document.activeElement as HTMLElement | null;
    const node = panel.current;
    // Not `a?.focus() ?? b.focus()`: `focus()` returns undefined, so the
    // fallback ran every time and pulled focus straight back to the panel.
    const firstControl = node?.querySelector<HTMLElement>(FOCUSABLE);
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
      // `offsetParent` would be the natural visibility test and is always
      // null in jsdom, which silently emptied this list and disabled the trap
      // under test. Attributes work in both.
      const focusable = [
        ...node.querySelectorAll<HTMLElement>(FOCUSABLE),
      ].filter(
        (el) =>
          !el.hasAttribute("hidden") &&
          el.getAttribute("aria-hidden") !== "true",
      );
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
      className={styles.overlay}
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
          styles.panel,
          wide === true ? styles.wide : "",
          tall === true ? styles.tall : "",
        ]
          .filter(Boolean)
          .join(" ")}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
      >
        <div className={styles.head}>
          {icon !== undefined && (
            <span className={styles.icon} aria-hidden="true">
              {icon}
            </span>
          )}
          <h2 className={styles.title}>{title}</h2>
          {actions}
        </div>
        <div className={styles.body}>{children}</div>
        {footer !== undefined && <div className={styles.foot}>{footer}</div>}
      </div>
    </div>
  );
}
