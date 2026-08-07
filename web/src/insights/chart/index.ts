// The chart wrapper's public surface: a drawable model, and the component that
// draws it — loaded only when a chart is actually on screen.
//
// The engine is behind `React.lazy` so ECharts is its own bundle chunk: a
// workspace that lives in Mail never downloads it, and Insights pays for it
// once, on the first board that has a bar on it. This is the same treatment the
// authoring editor gives KaTeX and Prism, for the same reason.
import { lazy } from "react";

export { align, chartModel } from "./model";
export type { AlignedSeries, ChartModel } from "./model";

/** One chart. Render it inside a `<Suspense>` — the fallback is on screen only
 *  for as long as the engine chunk takes to arrive. */
export const Chart = lazy(() => import("./EChart").then((m) => ({ default: m.EChart })));
