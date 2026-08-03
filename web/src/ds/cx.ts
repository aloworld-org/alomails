// Join class names into a single string, dropping falsy parts. Needed because
// CSS-module class access is typed `string | undefined` under
// noUncheckedIndexedAccess; cx() collapses that to a definite string for props
// (NavLink, custom components) that reject `undefined` under
// exactOptionalPropertyTypes.
export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
