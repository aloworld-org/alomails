// Stylesheets that still define a primitive the design system should own.
//
// **This list may only shrink.** `primitives.test.ts` fails when any *other*
// stylesheet defines `.button`, `.input`, `.field`, `.modal`, `.card`,
// `.table`, `.badge`, `.chip`, `.toolbar`, `.select`, `.checkbox` or
// `.toggle` — so nothing new can drift, and adopting a `ds/` component means
// deleting the local rules and then deleting the line here.
//
// # Why the list exists at all
//
// On 2026-08-12 there were 46 stylesheets carrying 136 such
// definitions: `.input` written 22 times, `.modal` 15, `.field` 16.
// Roughly a hundred hand-built copies of a dozen primitives, in a codebase
// whose token discipline is otherwise excellent — 7,422 `var(--token)`
// references against 108 hard-coded colours. The values were never the
// problem. The missing layer was.
//
// That is why the screens look subtly unlike each other, and why work keeps
// appearing whose entire purpose is to "unify" or "canonicalize" styling that
// will simply drift again. A convention cannot hold a line that a build does
// not check.
//
// # How a line leaves this list
//
// Adopt the `ds/` component, delete the local rules, delete the line. Each
// migration makes the next screen shorter, and the number below only goes
// down. Adding a line is a deliberate exemption and should be argued for; it
// is not a way to land a new hand-rolled input.
export const REDEFINES_PRIMITIVES: readonly string[] = [
  "inventory/InventoryModule.module.css",
  "platform/StackBadge.module.css",
  "sites/SitesModule.module.css",
] as const;
