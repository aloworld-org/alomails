// Showing and re-editing a catalog price, and nothing else.
//
// Prices arrive as integer minor units with the currency's own exponent
// beside them (the server sends both), so this module never guesses how many
// decimals a currency has and never *computes* with money: it formats what
// the store holds, and turns it back into the plain text an input can carry.
// Reading what a person typed is the server's job — its parser accepts
// `4.50`, `4,50` and `€4,50`, and a second, weaker copy here is how two doors
// end up disagreeing about a published price.
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
