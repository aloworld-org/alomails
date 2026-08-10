import { useEffect, useState } from "react";

/** The width at or below which the app switches to its single-pane,
 * phone-friendly layout. */
export const MOBILE_MAX_WIDTH = 768;

/**
 * Subscribes to a CSS media query and returns whether it currently
 * matches. SSR/again-safe: reads the initial value synchronously when a
 * window exists, and updates on change.
 */
export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() =>
    // `in window` is not enough: an environment can declare the property
    // without implementing it (jsdom does), and the call then throws where a
    // missing feature should simply mean "no".
    typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? window.matchMedia(query).matches
      : false,
  );

  useEffect(() => {
    if (
      typeof window === "undefined" ||
      typeof window.matchMedia !== "function"
    )
      return;
    const mql = window.matchMedia(query);
    const onChange = (e: MediaQueryListEvent) => setMatches(e.matches);
    // Sync in case it changed between the initial render and this effect.
    setMatches(mql.matches);
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, [query]);

  return matches;
}

/** Whether the viewport is phone-sized (the single-pane layout). */
export function useIsMobile(): boolean {
  return useMediaQuery(`(max-width: ${MOBILE_MAX_WIDTH}px)`);
}
