// Showing and re-editing a catalog price, and nothing else.
//
// Prices arrive as integer minor units with the currency's own exponent
// beside them (the server sends both), so this module never guesses how many
// decimals a currency has and never *computes* with money: it formats what
// the store holds, and turns it back into the plain text an input can carry.
// For the *catalog* routes, reading what a person typed is the server's job —
// its parser accepts `4.50`, `4,50` and `€4,50`, and a second, weaker copy
// here is how two doors end up disagreeing about a published price. The
// Billing routes take integer minor units instead, so the screens that apply
// through them carry the one inverse below — assembled from integer halves,
// never through a float, and refusing rather than rounding.
import { getLocale } from "../i18n";

/** The price as an editable string — `450` with exponent 2 becomes `4.50`,
 *  and no price at all stays empty. Deliberately locale-independent: it goes
 *  into a text input the server parses, not in front of a reader. */
export function priceInput(cents: number | null, exponent: number): string {
  if (cents === null) return "";
  if (exponent <= 0) return String(cents);
  const unit = 10 ** exponent;
  const whole = Math.trunc(cents / unit);
  const fraction = Math.abs(cents % unit);
  const sign = cents < 0 && whole === 0 ? "-" : "";
  return `${sign}${whole}.${String(fraction).padStart(exponent, "0")}`;
}

/** A typed amount as integer minor units — `4.50` or `4,50` with exponent 2
 *  becomes 450 — or `null` when the text is not a plain non-negative amount
 *  at that scale. Deliberately strict: one decimal digit too many, a stray
 *  character or a grouping separator is a refusal, not a silent rounding,
 *  because a number the user did not type is worse than a form that asks
 *  again. The two integer halves are assembled without ever passing through
 *  a float. */
export function parsePriceInput(text: string, exponent: number): number | null {
  const compact = text.replace(/\s/g, "");
  if (compact === "") return null;
  const match = /^([0-9]+)(?:[.,]([0-9]*))?$/.exec(compact);
  if (match === null) return null;
  const whole = match[1] ?? "";
  const fraction = match[2] ?? "";
  if (fraction.length > exponent) return null;
  const scaled =
    Number(whole) * 10 ** exponent + Number(fraction.padEnd(exponent, "0") || "0");
  return Number.isSafeInteger(scaled) ? scaled : null;
}

/** The price as a reader sees it, in their own language and the catalog's
 *  currency. The division is display-only and exact for every price the store
 *  accepts (at most 10 million major units); nothing stored or sent is ever a
 *  fraction. An unknown currency code formats as the code itself rather than
 *  throwing the screen away. */
export function formatPrice(cents: number, currency: string, exponent: number): string {
  const amount = exponent <= 0 ? cents : cents / 10 ** exponent;
  try {
    return new Intl.NumberFormat(getLocale(), { style: "currency", currency }).format(amount);
  } catch {
    return `${priceInput(cents, exponent)} ${currency}`;
  }
}
