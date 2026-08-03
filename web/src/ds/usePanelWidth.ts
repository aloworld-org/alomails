// Manages a resizable panel's width: bounded state, drag deltas, persistence
// across sessions, and reset-to-default. Used with <ResizeHandle>. Generic so
// any multi-pane module (Mail now, Agenda/Chat later) reuses it.
import { useCallback, useRef, useState } from "react";

/** Clamp a width to [min, max] — pure, unit-tested. */
export function clampWidth(px: number, minPx: number, maxPx: number): number {
  return Math.max(minPx, Math.min(maxPx, px));
}

export interface PanelWidth {
  /** Current width in px (already clamped). */
  width: number;
  /** Apply a drag delta (px). */
  applyDelta: (deltaX: number) => void;
  /** Persist the current width (call on drag end). */
  commit: () => void;
  /** Reset to the default and persist. */
  reset: () => void;
}

export function usePanelWidth(
  storageKey: string,
  defaultPx: number,
  minPx: number,
  maxPx: number,
): PanelWidth {
  const [width, setWidth] = useState<number>(() => {
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved !== null) {
        const n = Number.parseInt(saved, 10);
        if (Number.isFinite(n)) return clampWidth(n, minPx, maxPx);
      }
    } catch {
      // storage unavailable — fall through to default
    }
    return clampWidth(defaultPx, minPx, maxPx);
  });

  // Latest width, readable from the stable `commit` without stale closures.
  const widthRef = useRef(width);
  widthRef.current = width;

  const applyDelta = useCallback(
    (deltaX: number) => {
      setWidth((w) => clampWidth(w + deltaX, minPx, maxPx));
    },
    [minPx, maxPx],
  );

  const persist = useCallback(
    (value: number) => {
      try {
        localStorage.setItem(storageKey, String(value));
      } catch {
        // ignore — width simply won't survive reload
      }
    },
    [storageKey],
  );

  const commit = useCallback(() => persist(widthRef.current), [persist]);

  const reset = useCallback(() => {
    const value = clampWidth(defaultPx, minPx, maxPx);
    setWidth(value);
    persist(value);
  }, [defaultPx, minPx, maxPx, persist]);

  return { width, applyDelta, commit, reset };
}
