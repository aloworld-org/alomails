// Reading an Insights answer: what a figure says, what a bucket is called, and
// what the notes beside it mean (ADR 0037, wave BI1.05).
//
// Every function here formats ONE integer or ONE key the server sent. Nothing
// is summed, converted, scaled or re-derived — the tile and the VAT return
// agree because they are the same figure, computed once, on the server
// (`docs/design/insights.md` § Where money may be added up). A browser that
// added up a column here would be the first place the two could disagree.
//
// The labelling rule is the design note's, and it is why the server sends ids
// rather than words: a bucket from our closed vocabulary is translated here, in
// the reader's language; the tenant's own words (a customer, a stage, a payment
// method) are shown exactly as stored, because they were never ours; and a VAT
// rate is a number the reader's locale formats. A period key is ISO on the
// wire — `2026-01`, `2026-Q1`, `2026-W03`, `2026-01-15`, `2026` — and a date
// written in an English order is a bug in a European product.
import { formatAmount, formatRate } from "../billing";
import { getLocale, strings } from "../i18n";
import type { Series, SeriesGroup, SeriesLabel, SeriesNote, SeriesPoint } from "./types";

/** The bucket key of an answer with no breakdown: one figure, over the period. */
export const TOTAL_BUCKET = "total";
/** The bucket the folded tail of a category breakdown lands in. */
export const OTHER_BUCKET = "other";

/** The catalog ids the query engine sends, in the reader's language. An id this
 *  build does not know is shown as itself: a newer server's honest token beats
 *  calling something "Unknown" that is not. */
function catalogLabel(id: string): string {
  switch (id) {
    case "status.issued":
      return strings.insightsStatusIssued;
    case "status.paid":
      return strings.insightsStatusPaid;
    case "outcome.won":
      return strings.insightsOutcomeWon;
    case "outcome.lost":
      return strings.insightsOutcomeLost;
    case "outcome.open":
      return strings.insightsOutcomeOpen;
    case "age.not_due":
      return strings.insightsAgeNotDue;
    case "age.0_30":
      return strings.insightsAge0To30;
    case "age.31_60":
      return strings.insightsAge31To60;
    case "age.61_90":
      return strings.insightsAge61To90;
    case "age.90_plus":
      return strings.insightsAge90Plus;
    case "bucket.other":
      return strings.insightsBucketOther;
    case "series.all":
      return strings.insightsGroupAll;
    case "value.none":
      return strings.insightsValueNone;
    case "value.unknown":
      return strings.insightsValueUnknown;
    default:
      return id;
  }
}

/** What a group or a labelled bucket is called on screen. */
export function labelText(label: SeriesLabel): string {
  switch (label.kind) {
    case "catalog":
      return catalogLabel(label.id);
    case "raw":
      return label.text;
    case "rate_bp":
      return formatRate(label.bp, getLocale());
  }
}

/** An ISO period key in the reader's language.
 *
 *  Built from its parts rather than parsed as a date string: `new Date(
 *  "2026-01-15")` is an *instant* at UTC midnight, which reads as the 14th for
 *  anybody west of Greenwich — and a month that slips a day at a boundary is a
 *  wrong figure under a right heading. A key this build does not recognise is
 *  shown verbatim rather than guessed at. */
export function bucketLabel(bucket: string): string {
  const locale = getLocale();
  const day = /^(\d{4})-(\d{2})-(\d{2})$/.exec(bucket);
  if (day !== null) {
    return new Date(Number(day[1]), Number(day[2]) - 1, Number(day[3])).toLocaleDateString(locale, {
      day: "numeric",
      month: "short",
      year: "numeric",
    });
  }
  const month = /^(\d{4})-(\d{2})$/.exec(bucket);
  if (month !== null) {
    return new Date(Number(month[1]), Number(month[2]) - 1, 1).toLocaleDateString(locale, {
      month: "short",
      year: "numeric",
    });
  }
  const quarter = /^(\d{4})-Q(\d)$/.exec(bucket);
  if (quarter !== null) return strings.insightsQuarter(Number(quarter[2]), Number(quarter[1]));
  const week = /^(\d{4})-W(\d{2})$/.exec(bucket);
  if (week !== null) return strings.insightsWeek(Number(week[2]), Number(week[1]));
  if (/^\d{4}$/.test(bucket)) return bucket;
  if (bucket === TOTAL_BUCKET) return strings.insightsBucketTotal;
  if (bucket === OTHER_BUCKET) return strings.insightsBucketOther;
  return bucket;
}

/** What one point is called: its own label when it has one (a category), the
 *  formatted period when it does not (a time bucket says everything in its
 *  key). */
export function pointLabel(point: SeriesPoint): string {
  return point.label === undefined ? bucketLabel(point.bucket) : labelText(point.label);
}

/**
 * One figure, read in the interface language.
 *
 * The unit decides everything: cents become money in the currency the whole
 * answer is stated in — or, when money could not honestly be restated into one,
 * the currency of the group the figure belongs to (`docs/design/insights.md`
 * § Deals are never converted); a count is a plain number; a ratio is basis
 * points shown as a percentage. The value itself is never touched.
 */
export function figureText(series: Series, group: SeriesGroup, value: number): string {
  const locale = getLocale();
  switch (series.unit.kind) {
    case "money":
      return formatAmount(value, locale, series.unit.currency ?? group.key);
    case "percent_bp":
      return formatRate(value, locale);
    case "count":
      return new Intl.NumberFormat(locale).format(value);
  }
}

/** The short form an axis tick shows: the same figure, without the currency
 *  symbol repeated on every gridline (the tile's subtitle already says which
 *  currency the answer is in). */
export function axisText(series: Series, value: number): string {
  const locale = getLocale();
  if (series.unit.kind === "percent_bp") return formatRate(value, locale);
  if (series.unit.kind === "count") return new Intl.NumberFormat(locale).format(value);
  return formatAmount(value, locale);
}

/** What a note means, in the reader's language. An unknown code is dropped
 *  rather than shown as a token: a caption nobody can read is not honesty. */
export function noteText(note: SeriesNote): string | null {
  if (note.code === "unconverted_documents") return strings.insightsNoteUnconverted(note.count);
  return null;
}

/** Whether an answer has any figure in it at all — an empty series is a real
 *  answer ("nothing was billed"), and the tile says so in words rather than
 *  drawing an empty chart. */
export function hasFigures(series: Series): boolean {
  return series.series.some((group) => group.points.length > 0);
}
