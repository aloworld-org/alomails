// Drawing an answer (ADR 0037): a series, the form it is drawn in, and the
// renderer that suits both.
//
// It is its own file because two screens draw the same answer — a tile pinned
// to a board, and the preview of a chart the assistant has proposed but nobody
// has pinned yet (BI1.07). A preview that drew its figures differently from the
// tile it becomes would be a demonstration of something the reader is not
// actually getting.
//
// Nothing here computes a figure: every value was computed by the server, and
// this only chooses which component puts it on the screen.
import { strings } from "../i18n";
import { ChartFigure } from "./ChartFigure";
import type { DrawnViz } from "./ChartFigure";
import { hasFigures } from "./format";
import { NumberFigure } from "./NumberFigure";
import { TableFigure } from "./TableFigure";
import type { Series, Viz } from "./types";
import styles from "./InsightsModule.module.css";

/** Draws `series` in the form `viz` names. A bar is what anything unknown is
 *  drawn as, because a chart nobody can see is a worse answer than a plain one.
 *
 *  An answer with no figures in it is a real answer — nothing was billed in
 *  that period — and says so rather than drawing an empty canvas. */
export function Figures({
  series,
  viz,
  title,
}: {
  series: Series;
  viz: Viz | null;
  title: string;
}) {
  if (!hasFigures(series))
    return <p className={styles.quiet}>{strings.insightsNoFigures}</p>;
  if (viz === "number") return <NumberFigure series={series} />;
  if (viz === "table") return <TableFigure series={series} label={title} />;
  const drawn: DrawnViz =
    viz === "line" ? "line" : viz === "pie" ? "pie" : "bar";
  return <ChartFigure series={series} viz={drawn} title={title} />;
}
