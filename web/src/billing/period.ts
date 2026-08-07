// The reporting periods a bookkeeper actually asks for (alo Billing, ADR 0035,
// wave B1.20), as plain `YYYY-MM-DD` days.
//
// A period here is a **calendar** fact, not an instant: "the third quarter"
// means 1 July to 30 September everywhere, in every time zone. So the days are
// built and compared as text and as UTC date parts, never as local timestamps —
// the same rule `dates.ts` follows for a document's own dates, and for the same
// reason (a quarter that starts on 30 June for a reader west of Greenwich is a
// wrong VAT return).
//
// Nothing here decides what a period *contains*: the server does that, from the
// issue date frozen on each document. These are only the two days a form is
// prefilled with, so the user is not made to type the obvious.

/** A period as the API takes it: two `YYYY-MM-DD` days, both included. */
export interface Period {
  from: string;
  to: string;
}

/** A UTC date as `YYYY-MM-DD`. */
function day(date: Date): string {
  const year = String(date.getUTCFullYear()).padStart(4, "0");
  const month = String(date.getUTCMonth() + 1).padStart(2, "0");
  const dayOfMonth = String(date.getUTCDate()).padStart(2, "0");
  return `${year}-${month}-${dayOfMonth}`;
}

/**
 * The calendar quarter `today` falls in — the period a VAT return is nearly
 * always for, and therefore the one the form opens on.
 *
 * `today` is passed in rather than read here, so the caller decides which clock
 * is authoritative and the function stays testable.
 */
export function quarterOf(today: Date): Period {
  const year = today.getUTCFullYear();
  const firstMonth = Math.floor(today.getUTCMonth() / 3) * 3;
  return {
    from: day(new Date(Date.UTC(year, firstMonth, 1))),
    // Day zero of the month after the quarter is the last day of the quarter,
    // which is how the month lengths and the leap years stay somebody else's
    // problem.
    to: day(new Date(Date.UTC(year, firstMonth + 3, 0))),
  };
}

/** The quarter before the one `today` falls in — the period a return is
 *  actually filed for, since a quarter is declared once it has ended. */
export function previousQuarterOf(today: Date): Period {
  const firstOfThis = new Date(`${quarterOf(today).from}T00:00:00Z`);
  firstOfThis.setUTCDate(0); // the last day of the previous quarter
  return quarterOf(firstOfThis);
}
