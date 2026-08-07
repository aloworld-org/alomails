// A server answer, turned into the neutral thing a chart draws (ADR 0037, wave
// BI1.05).
//
// This file exists so that the *shape* of a chart is decided in code we own and
// can test, rather than inside the chart library's option object. It knows
// nothing about ECharts — no import, no type — and the renderer next door knows
// nothing about the wire format. Swapping the engine is then one file
// (`EChart.tsx`), which is the rule the design note sets for keeping a chart
// library a dependency rather than an architecture.
//
// Nothing here computes a figure. It orders buckets, aligns the groups against
// a shared axis, and asks `format.ts` for the words — every `value` is the
// integer the server sent, carried through untouched.
import { axisText, figureText, labelText, pointLabel } from "../format";
import type { Series } from "../types";

/** One drawn point: the value the axis uses, and the text a reader sees. */
export interface ChartValue {
  /** The figure, or `null` where this group had no answer for that bucket —
   *  a gap the chart leaves blank rather than drawing as a zero nobody
   *  measured. */
  value: number | null;
  /** The formatted figure, for the tooltip and the pie's labels. */
  text: string;
}

/** One line, bar set or pie, aligned to the model's categories. */
export interface ChartSeries {
  key: string;
  name: string;
  values: ChartValue[];
}

/** The groups of an answer, aligned against one shared list of buckets — what
 *  a chart draws and what a table lists, which are the same thing seen twice. */
export interface AlignedSeries {
  /** The buckets, in the order the server returned them — the sort is the
   *  server's, and re-sorting here would answer a different question. */
  categories: string[];
  series: ChartSeries[];
  /** Whether more than one group is present — a legend (or a table column per
   *  group) is worth its space only then, and money in two currencies is
   *  exactly when it happens. */
  multi: boolean;
}

/** An aligned answer, plus how it is drawn. */
export interface ChartModel extends AlignedSeries {
  kind: "bar" | "line" | "pie";
  /** An axis tick, formatted. */
  axisLabel: (value: number) => string;
}

/**
 * The buckets to draw, in order: the first group's, then anything a later group
 * has that the first did not.
 *
 * Union rather than intersection, because a bucket that only one currency has
 * is still a fact — dropping it would hide a month somebody was paid in
 * dollars.
 */
function categoriesOf(series: Series): string[] {
  const keys: string[] = [];
  for (const group of series.series) {
    for (const point of group.points) {
      if (!keys.includes(point.bucket)) keys.push(point.bucket);
    }
  }
  return keys;
}

/**
 * The answer's groups, aligned against one list of buckets and formatted.
 *
 * `only` narrows it to one group — how a pie stays honest when money comes back
 * in two currencies: shares of a whole are shares of *one* whole, so the caller
 * draws one pie per currency rather than one pie mixing them.
 */
export function align(series: Series, only?: string): AlignedSeries {
  const groups = only === undefined ? series.series : series.series.filter((g) => g.key === only);
  const buckets = categoriesOf({ ...series, series: groups });
  const labels = new Map<string, string>();
  for (const group of groups) {
    for (const point of group.points) labels.set(point.bucket, pointLabel(point));
  }
  return {
    categories: buckets.map((key) => labels.get(key) ?? key),
    series: groups.map((group) => ({
      key: group.key,
      name: labelText(group.label),
      values: buckets.map((bucket) => {
        const point = group.points.find((p) => p.bucket === bucket);
        return point === undefined
          ? { value: null, text: "" }
          : { value: point.value, text: figureText(series, group, point.value) };
      }),
    })),
    multi: groups.length > 1,
  };
}

/** A drawable model of `series`, for the viz named by `kind`. */
export function chartModel(series: Series, kind: ChartModel["kind"], only?: string): ChartModel {
  return { ...align(series, only), kind, axisLabel: (value: number) => axisText(series, value) };
}
