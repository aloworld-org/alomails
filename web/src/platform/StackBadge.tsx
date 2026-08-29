// A small, permanent marker in dev builds saying which stack is on screen.
//
// Deliberately not dismissible. The whole value is that it is there at the
// moment somebody has stopped thinking about ports and started debugging a
// "bug" — which is exactly when a dismissed badge would have been dismissed.
//
// Deliberately not translated, and this does not breach the catalog rule: it
// adds no key to `strings`. No user ever sees it, and three translations of a
// developer's diagnostic would be three strings nobody reads and everybody has
// to maintain.
import { API_BASE } from "./runtime";
import { stackLabel } from "./stack";

// Bottom-right, above everything, and out of the way of the rail and the
// composer. Small enough to ignore while working, findable the moment
// somebody asks "which server am I on?".
//
// The black scrim and white ink are literal on purpose: this badge must look
// identical whatever theme is active — it identifies the stack even when the
// theming is the thing being debugged — so it takes no colour from the theme.
// `pointer-events-none` so it never eats a click meant for the app underneath.
const badge =
  "pointer-events-none fixed right-[6px] bottom-[6px] z-[2147483647] " +
  "select-none rounded-sm bg-[rgb(0_0_0/0.72)] px-[7px] py-[2px] font-mono " +
  "text-[11px] leading-normal tracking-[0.02em] text-[#fff]";

export function StackBadge() {
  // `import.meta.env.DEV` is replaced at build time and the whole component
  // drops out of a production bundle. Belt and braces: `stackLabel` also
  // refuses any non-local origin, so even a mistaken render on a deployed
  // build shows nothing.
  if (!import.meta.env.DEV) return null;
  const label = stackLabel(window.location.origin, API_BASE);
  if (label === null) return null;
  return (
    <div className={badge} title={label.detail} aria-hidden="true">
      {label.text}
    </div>
  );
}
