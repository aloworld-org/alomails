// A draggable vertical divider between two panels. It reports drag deltas to
// the caller (which updates the panel width via usePanelWidth), supports
// keyboard resize (arrow keys) for accessibility, and double-click to reset.
// The visible divider is 1px; the grab area is widened invisibly so it is easy
// to catch.
import { useRef, useState } from "react";
import type { KeyboardEvent, PointerEvent } from "react";

import { cx } from "./cx";

// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046).
// The build that replaced this file's stylesheet changed no rule.

/** A 1px divider that is also the drag target. The `::before` widens the hit
 *  area to ~9px without changing layout — 4px either side of a 1px rule — and
 *  is the one part of this drawing with proportions of its own, so those stay
 *  literals. `touch-action-none` keeps a drag from scrolling the pane under
 *  the finger. */
const BASE =
  "basis-px grow-0 shrink-0 self-stretch relative bg-subtle " +
  "cursor-col-resize touch-none " +
  "transition-colors duration-[var(--duration-fast)] ease-standard " +
  "before:content-[''] before:absolute before:inset-y-0 before:-inset-x-1 " +
  "before:z-5 " +
  // Neutral highlight, never the accent: a divider you are holding should be
  // legible, not decorated. Focus replaces the global outline for the same
  // reason — a 1px rule with a 2px outline around it is mostly outline.
  "hover:bg-strong focus-visible:outline-none focus-visible:bg-tertiary";

/** Held. Same fill as hover, applied while the pointer is down anywhere. */
const DRAGGING = "bg-strong";

interface ResizeHandleProps {
  /** Called with the horizontal drag delta in px. */
  onResize: (deltaX: number) => void;
  /** Called when a drag or keyboard adjust finishes (persist here). */
  onCommit?: () => void;
  /** Double-click action (reset to default width). */
  onReset?: () => void;
  /** Accessible name, e.g. "Resize the folders panel". */
  ariaLabel: string;
}

const KEYBOARD_STEP = 16;

export function ResizeHandle({
  onResize,
  onCommit,
  onReset,
  ariaLabel,
}: ResizeHandleProps) {
  const [dragging, setDragging] = useState(false);
  const lastX = useRef(0);

  function beginDrag(e: PointerEvent<HTMLDivElement>) {
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    lastX.current = e.clientX;
    setDragging(true);
    // Prevent text selection + keep the resize cursor for the whole drag.
    document.body.style.userSelect = "none";
    document.body.style.cursor = "col-resize";
  }

  function moveDrag(e: PointerEvent<HTMLDivElement>) {
    if (!dragging) return;
    const dx = e.clientX - lastX.current;
    if (dx !== 0) {
      lastX.current = e.clientX;
      onResize(dx);
    }
  }

  function endDrag(e: PointerEvent<HTMLDivElement>) {
    if (!dragging) return;
    setDragging(false);
    document.body.style.userSelect = "";
    document.body.style.cursor = "";
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    onCommit?.();
  }

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (e.key === "ArrowLeft") {
      onResize(-KEYBOARD_STEP);
      onCommit?.();
      e.preventDefault();
    } else if (e.key === "ArrowRight") {
      onResize(KEYBOARD_STEP);
      onCommit?.();
      e.preventDefault();
    }
  }

  return (
    <div
      className={cx(BASE, dragging && DRAGGING)}
      role="separator"
      aria-orientation="vertical"
      aria-label={ariaLabel}
      tabIndex={0}
      onPointerDown={beginDrag}
      onPointerMove={moveDrag}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onDoubleClick={() => onReset?.()}
      onKeyDown={onKeyDown}
    />
  );
}
