// A draggable vertical divider between two panels. It reports drag deltas to
// the caller (which updates the panel width via usePanelWidth), supports
// keyboard resize (arrow keys) for accessibility, and double-click to reset.
// The visible divider is 1px; the grab area is widened invisibly so it is easy
// to catch.
import { useRef, useState } from "react";
import type { KeyboardEvent, PointerEvent } from "react";

import { cx } from "./cx";
import styles from "./ResizeHandle.module.css";

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
      className={cx(styles.handle, dragging && styles.dragging)}
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
