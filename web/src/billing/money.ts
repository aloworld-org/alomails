// Reading and writing the scaled integers a billing form takes: a unit price
// (integer cents), a VAT rate (integer basis points) and a line quantity
// (integer milli-units). Every one of them is "a decimal the API wants as a
// whole number of some fixed fraction", so one parser and one formatter serve
// all three -- there is no second, slightly different rule to drift.
//
// This file does NOT compute money. Totals, VAT breakdowns and everything else
// derived come from the server (`docs/design/billing.md`), so the client can
// never disagree with the document. What lives here is strictly the edge where
// a human types "1.234,56" and the API wants `123456`, and back again.
//
// The parse rule is deliberately locale-independent, because a Dutch user with
// an English UI still types Dutch numbers:
//   - whitespace (including the non-breaking and thin spaces a paste from a
//     spreadsheet carries) is a grouping separator and is dropped;
//   - if BOTH `.` and `,` appear, the last one is the decimal separator;
//   - if only one kind appears, exactly once, followed by one or two digits,
//     it is the decimal separator ("1,50" and "1.50" both mean one and a half);
//   - otherwise every separator is a grouping separator, and the integer part
//     must really be grouped in threes ("1.500" and "1,500" both mean fifteen
//     hundred; "1.2345" is refused, not read as a number nobody typed).
// Anything else -- letters, a repeated decimal separator, one decimal digit
// more than the scale allows -- is `null`, and the form says so rather than
// storing a guess.
//
// A **quantity** (three decimals) takes no grouping at all, which is the one
// place the two scales differ. "1.500" as an amount is unambiguously fifteen
// hundred, but as a quantity it is one and a half just as often as it is
// fifteen hundred, and 0.125 hours has to stay writable. Rather than guess
// between them, a quantity reads every separator as the decimal point and
// refuses a grouped integer part -- so what a document is billed for is never
// a thousand times what someone typed.

/** The decimal separators a user may type. */
const SEPARATORS = [".", ","] as const;

/** Every space a pasted number can carry. `\s` already covers the non-breaking
 *  and thin spaces a spreadsheet groups with; the zero-width space does not
 *  count as whitespace in JavaScript and has to be named. */
const SPACES = /[\s\u200b]/g;

/** The integer part with no grouping at all. */
const PLAIN_DIGITS = /^[0-9]*$/;

/** The integer part grouped in threes under one consistent separator. */
const GROUPED_DIGITS = /^(?:[0-9]{1,3}(?:\.[0-9]{3})+|[0-9]{1,3}(?:,[0-9]{3})+)$/;

/**
 * Parses a typed decimal into whole units of `10^-decimals` -- hundredths for
 * an amount or a rate, thousandths for a quantity -- or `null` when the text
 * is not a number at that scale.
 *
 * `grouping` says whether the thousands separators of a written amount are
 * recognised; it is off for a quantity, where they cannot be told apart from a
 * decimal point (see the note at the top of this file).
 *
 * Never rounds: one decimal digit too many is a refusal, not a silent
 * truncation, because a number the user did not type is worse than a form that
 * asks again.
 */
export function parseScaled(text: string, decimals: number, grouping = true): number | null {
  const compact = text.replace(SPACES, "");
  if (compact === "") return null;

  const sign = compact.startsWith("-") ? -1 : 1;
  const body = compact.replace(/^[+-]/, "");
  if (!/^[0-9.,]*$/.test(body) || !/[0-9]/.test(body)) return null;

  const decimal = decimalSeparator(body, decimals, grouping);
  const parts = decimal === null ? [body] : body.split(decimal);
  if (parts.length > 2) return null; // the same separator twice: not a number
  const integerPart = parts[0] ?? "";
  const fraction = parts[1] ?? "";
  if (fraction.length > decimals || !PLAIN_DIGITS.test(fraction)) return null;
  if (!PLAIN_DIGITS.test(integerPart) && !(grouping && GROUPED_DIGITS.test(integerPart))) {
    return null;
  }

  const whole = SEPARATORS.reduce((acc, s) => acc.split(s).join(""), integerPart);
  if (whole === "" && fraction === "") return null;

  // Assembled from the two integer halves rather than parsed as a float: a
  // price must never depend on whether `1.15 * 100` lands on 115 or 114.999.
  const scale = 10 ** decimals;
  const scaled =
    (whole === "" ? 0 : Number(whole)) * scale + Number(fraction.padEnd(decimals, "0") || "0");
  return Number.isSafeInteger(scaled) ? sign * scaled : null;
}

/** A typed amount or rate as whole hundredths: cents, or basis points. */
export function parseHundredths(text: string): number | null {
  return parseScaled(text, 2);
}

/** A typed quantity as whole milli-units, the scale a document line uses
 *  (1.5 hours = 1500), so a third of an hour is exact. Grouping separators are
 *  not read here: "1.500" is one and a half. */
export function parseMilli(text: string): number | null {
  return parseScaled(text, 3, false);
}

/** Which of `.` / `,` in `body` is the decimal separator, if either is. */
function decimalSeparator(body: string, decimals: number, grouping: boolean): string | null {
  const present = SEPARATORS.filter((s) => body.includes(s));
  if (present.length === 2) {
    // Mixed notation ("1.234,56" / "1,234.56"): the last one decides.
    const [first, second] = present;
    if (first === undefined || second === undefined) return null;
    return body.lastIndexOf(first) > body.lastIndexOf(second) ? first : second;
  }
  const only = present[0];
  if (only === undefined) return null;
  if (body.indexOf(only) !== body.lastIndexOf(only)) return null; // repeated: grouping
  const after = body.length - body.indexOf(only) - 1;
  // With grouping off there is nothing else a separator could be, so the only
  // question is whether the tail fits the scale.
  if (!grouping) return after >= 1 && after <= decimals ? only : null;
  return after === 1 || after === 2 ? only : null;
}

/**
 * The editable form of a scaled integer: a plain number with a `.` decimal
 * separator and no grouping, so a prefilled field always parses back to
 * exactly the value it came from. Trailing zeros are dropped -- a 21 % rate
 * reads `21`, not `21.00`.
 */
export function scaledToInput(value: number, decimals: number): string {
  const sign = value < 0 ? "-" : "";
  const abs = Math.abs(value);
  const scale = 10 ** decimals;
  const fraction = abs % scale;
  const whole = (abs - fraction) / scale;
  if (fraction === 0) return `${sign}${whole}`;
  const digits = String(fraction).padStart(decimals, "0").replace(/0+$/, "");
  return `${sign}${whole}.${digits}`;
}

/** The editable form of whole hundredths -- an amount, or a rate. */
export function hundredthsToInput(value: number): string {
  return scaledToInput(value, 2);
}

/** The editable form of a quantity in milli-units (1500 -> "1.5"). */
export function milliToInput(value: number): string {
  return scaledToInput(value, 3);
}

/**
 * An amount for reading: grouped and always two decimals, in `locale`'s
 * convention. `currency` renders the symbol; omit it for the price list, which
 * is quoted in the tenant's own currency and carries no per-row currency.
 */
export function formatAmount(cents: number, locale: string, currency?: string): string {
  const options: Intl.NumberFormatOptions =
    currency === undefined
      ? { minimumFractionDigits: 2, maximumFractionDigits: 2 }
      : { style: "currency", currency, minimumFractionDigits: 2, maximumFractionDigits: 2 };
  try {
    return new Intl.NumberFormat(locale, options).format(cents / 100);
  } catch {
    // An unknown currency code (the server validates shape, not the ISO list)
    // must not blank a price list.
    return hundredthsToInput(cents);
  }
}

/** A quantity for reading: milli-units in `locale`'s convention, with only the
 *  decimals it actually has ("1500" -> "1.5", "2000" -> "2"). */
export function formatQty(qtyMilli: number, locale: string): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 3 }).format(qtyMilli / 1000);
}

/** A VAT rate for reading: basis points as a percentage ("2100" -> "21%"), with
 *  the spacing each language puts before its percent sign. */
export function formatRate(basisPoints: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    style: "percent",
    maximumFractionDigits: 2,
  }).format(basisPoints / 10000);
}
