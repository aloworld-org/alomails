// Close a popover when the user clicks away from it or presses Escape.
//
// Every menu, picker and popover needs this, and the ones that skip it are the
// ones that feel broken: two menus open at once, or one left hanging after the
// user has plainly moved on. Written once here so a new popover gets the
// behaviour by asking for it rather than by remembering to reimplement it.
//
// `pointerdown`, not `click`: a click fires after the press has already moved
// focus, which lets a popover swallow the very press meant to dismiss it.
import { useEffect } from "react";
import type { RefObject } from "react";

/**
 * While `open`, dismiss when a pointer goes down outside `ref`, or Escape is
 * pressed.
 *
 * `onDismiss` must be stable, or the listeners are torn down and rebuilt on
 * every render — pass a `useCallback` or a setter.
 */
export function useDismiss(
  open: boolean,
  ref: RefObject<HTMLElement | null>,
  onDismiss: () => void,
): void {
  useEffect(() => {
    if (!open) return undefined;
    function onPointerDown(event: PointerEvent) {
      const node = ref.current;
      if (node !== null && !node.contains(event.target as Node)) onDismiss();
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") onDismiss();
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, ref, onDismiss]);
}
