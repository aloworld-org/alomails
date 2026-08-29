// The behaviour of a modal layer, separated from the drawing of one.
//
// `Modal` draws a centred panel with a heading; the task detail is a slide-over
// anchored to the right whose "title" is an editable input. Teaching `Modal` a
// `side` variant would have replaced its header, its width, its height and its
// radius — everything but this file — so the drawing stays with each caller and
// the behaviour that must never be reimplemented lives here once: focus moves
// in on open and back to the opener on close, Tab cannot leave, and Escape
// closes the top layer only.
//
// The stack is module-level on purpose (D2.11): a layer can open another —
// Tasks' create form opens the Drive picker, the detail panel may open a
// dialog — and both listen on the document, where `stopPropagation` does not
// stop the other listener on the same node. The top of the stack alone answers
// Escape and owns the tab trap.
import { useEffect, useRef, type RefObject } from "react";

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

/** The open layers, bottom to top. */
const openLayers: HTMLElement[] = [];

/**
 * Make the element in `panel` behave as a modal layer: focus moves into it on
 * mount and back to the opener on unmount, Tab wraps inside it, and Escape
 * calls `onClose` — for the top layer of the stack only.
 *
 * The panel element is read once, on mount, so the ref must be attached to a
 * node that lives as long as the layer does — swap the *contents* of the
 * panel while it loads, never the panel itself.
 */
export function useModalStack(
  panel: RefObject<HTMLElement | null>,
  onClose: () => void,
): void {
  // The close handler is read through a ref at key time rather than being a
  // dependency of the effect: callers pass inline arrows, and re-running the
  // effect on every parent render would re-push the panel and yank focus back
  // to its first control in the middle of an edit.
  const closeRef = useRef(onClose);
  useEffect(() => {
    closeRef.current = onClose;
  }, [onClose]);

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
    if (node !== null) openLayers.push(node);

    function onKey(event: KeyboardEvent) {
      // Not the top of the stack → the key belongs to the layer above.
      if (node !== null && openLayers[openLayers.length - 1] !== node) return;
      if (event.key === "Escape") {
        event.stopPropagation();
        closeRef.current();
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
      if (node !== null) {
        const at = openLayers.lastIndexOf(node);
        if (at !== -1) openLayers.splice(at, 1);
      }
      opener?.focus?.();
    };
  }, [panel]);
}
