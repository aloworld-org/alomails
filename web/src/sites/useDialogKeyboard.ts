// What a dialog on this surface owes somebody who is not holding a mouse.
//
// Found by the wave review (S2.16b): every sites dialog was announced as
// `role="dialog" aria-modal="true"` and none of them behaved like one. Focus
// stayed on the button that opened the dialog, so the first Tab went to
// whatever followed that button *behind* the scrim; Tab kept walking through
// the page underneath, which a screen reader reads out happily even though it
// is covered and cannot be clicked; Escape was handled on the panel itself, so
// it did nothing until focus had already been moved inside by hand; and
// closing dropped the caret at the top of the document rather than back where
// the user was. Five dialogs autofocused their first field and so accidentally
// escaped the first symptom; the other seven did not.
//
// `ds/Modal.tsx` owns the canonical version of this behaviour. This is
// deliberately a second copy and not an import: ADR 0045 keeps the design
// system track out of `web/src/sites/**`, so this module carries its own
// dialog chrome, and the behaviour has to travel with the chrome. Keep the two
// in step.
import { useEffect, useRef, type RefObject } from "react";

/** Everything that can hold focus inside a panel, in tab order. */
const FOCUSABLE =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** The focusable controls of `node`, minus the ones hidden from either the
 *  screen or assistive technology.
 *
 *  `offsetParent` would be the natural visibility test and is always null in
 *  jsdom, which would silently empty this list and disable the trap under
 *  test — the attributes work in both. */
function focusableWithin(node: HTMLElement): HTMLElement[] {
  return [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) =>
      !el.hasAttribute("hidden") && el.getAttribute("aria-hidden") !== "true",
  );
}

/**
 * Gives a modal panel the keyboard contract its `role="dialog"` promises:
 * focus moves in when it opens, Tab cannot leave it, Escape closes it from
 * anywhere — including the instant it opens, before anything inside has been
 * touched — and the control that opened it gets focus back when it closes.
 *
 * @param panel the element carrying `role="dialog"`; give it `tabIndex={-1}`
 *   so it can hold focus itself when it contains no controls at all.
 * @param onClose what Escape means. May be a fresh closure on every render:
 *   the hook reads it through a ref rather than a dependency, so a re-render
 *   of the surrounding screen cannot re-run the effect and yank focus back to
 *   the first control while the user is typing in the third.
 */
export function useDialogKeyboard(
  panel: RefObject<HTMLElement | null>,
  onClose: () => void,
): void {
  const close = useRef(onClose);
  close.current = onClose;

  // Read during render, not in the effect: React applies a child's `autoFocus`
  // during the same commit, before this parent's effects run, so by effect
  // time `document.activeElement` in an autofocusing dialog is already the
  // field inside it and the opener would be lost. Render happens before the
  // panel is in the document, so this is still the element the user left.
  const opener = useRef<Element | null>(null);
  opener.current ??= document.activeElement;

  useEffect(() => {
    const node = panel.current;

    // Only if a child has not claimed focus already (`autoFocus` on the field
    // a dialog is really about beats the close button this would pick).
    if (node !== null && !node.contains(document.activeElement)) {
      const first = focusableWithin(node)[0];
      if (first !== undefined) first.focus();
      else node.focus();
    }

    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.stopPropagation();
        close.current();
        return;
      }
      if (event.key !== "Tab" || node === null) return;
      const focusable = focusableWithin(node);
      if (focusable.length === 0) return;
      const first = focusable[0]!;
      const last = focusable[focusable.length - 1]!;
      const active = document.activeElement;
      // Tab from the last control returns to the first and Shift+Tab from the
      // first goes to the last, so focus cannot leave a dialog that is
      // covering the page. Focus that is somehow outside the panel entirely —
      // the browser restored it, an extension moved it — is pulled back in
      // rather than left loose behind the scrim.
      if (!node.contains(active)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
      } else if (event.shiftKey && active === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    }

    // Capture, on the document: Escape has to work while focus is still on the
    // opener behind the scrim, which no listener on the panel can see.
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      const back = opener.current;
      // Not if the opener went away with the thing that was closed — focusing
      // a detached node silently moves focus to the body, which is where it
      // already is.
      if (back instanceof HTMLElement && back.isConnected) back.focus();
    };
  }, [panel]);
}
