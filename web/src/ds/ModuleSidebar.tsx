// The one module sidebar.
//
// Every module with its own sidebar was solving the phone the same way — Mail
// wrote an off-canvas drawer, Tasks copied it — and the copies were already
// diverging: neither could be closed from the keyboard, and Tab walked
// straight out of both onto the content behind. The same argument that
// produced the one Modal (ADR 0045) applies to the drawer, so here it is
// written once: at desktop widths the module's own sidebar column renders
// untouched, and at phone widths it becomes an off-canvas drawer with the
// backdrop, the focus trap and Escape-to-close inherited rather than
// remembered.
//
// The geometry is the Mail drawer's — left edge, `min(82vw, 20rem)` wide,
// laid over the content — and the trap is the Modal's. The module that mounts
// this must be a containing block (`position: relative`), which every module
// root already is.
import { useEffect, useRef, type ReactNode } from "react";

import { useIsMobile } from "./useMediaQuery";
import { MODAL_BACKDROP_CLASS } from "./modalBackdrop";

export interface ModuleSidebarProps {
  /** Whether the drawer is open. Only read at phone widths — the desktop
   *  column is always on screen and ignores it. */
  open: boolean;
  /** Called when the user dismisses the drawer: backdrop tap or Escape. Must
   *  be stable (a `useCallback` or a setter), or the trap is torn down and
   *  focus is re-seized on every render. The caller should also close on
   *  selection — picking a destination is the reason the drawer was opened. */
  onClose: () => void;
  /** Names the drawer for assistive technology. */
  label: string;
  children: ReactNode;
}

/** Everything that can hold focus inside the drawer, in tab order. The same
 *  selector and visibility filter as the Modal's, for the same reasons. */
const FOCUSABLE =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusableIn(node: HTMLElement): HTMLElement[] {
  return [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) =>
      !el.hasAttribute("hidden") && el.getAttribute("aria-hidden") !== "true",
  );
}

/** The drawer's own chrome, mounted only while it is open on a phone, so the
 *  trap effect runs exactly for the drawer's lifetime. */
function Drawer({
  onClose,
  label,
  children,
}: Pick<ModuleSidebarProps, "onClose" | "label" | "children">) {
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Where focus was, so closing the drawer puts the keyboard user back on
    // the toggle they pressed rather than at the top of the document.
    const opener = document.activeElement as HTMLElement | null;
    const node = panel.current;
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
      // The trap: focus cannot leave a drawer that is covering the content.
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
    <>
      <div
        className={`absolute inset-0 z-[calc(var(--z-overlay)-1)] bg-overlay ${MODAL_BACKDROP_CLASS}`}
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        ref={panel}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        tabIndex={-1}
        className="absolute inset-y-0 left-0 z-[var(--z-overlay)] flex w-[min(82vw,20rem)] shadow-lg *:min-w-0"
      >
        {children}
      </div>
    </>
  );
}

/**
 * A module's sidebar: the module's own column at desktop widths, an
 * off-canvas drawer at phone widths.
 *
 * The child is the fully styled column and should fill the drawer when it is
 * one (`width: 100%` at phone widths); the drawer decides the outer width,
 * position and shadow, and owns the backdrop, the focus trap and
 * Escape-to-close.
 */
export function ModuleSidebar({
  open,
  onClose,
  label,
  children,
}: ModuleSidebarProps) {
  const isMobile = useIsMobile();
  if (!isMobile) return <>{children}</>;
  if (!open) return null;
  return (
    <Drawer onClose={onClose} label={label}>
      {children}
    </Drawer>
  );
}
