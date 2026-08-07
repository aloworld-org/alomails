// The **only** file in alo that imports a chart library (ADR 0037,
// `docs/design/insights.md` § Chart rendering).
//
// Apache ECharts, Apache-2.0, bundled — no CDN, no map tiles, no telemetry, no
// network at runtime. Only the pieces we draw with are imported (bar, line,
// pie, a cartesian grid, a tooltip, a legend, the canvas renderer); the
// geo/map components that are the bulk of the library never are, and the whole
// file is loaded lazily by `chart/index.ts`, so a workspace that never opens
// Insights never downloads a chart engine.
//
// Everything above this file speaks `ChartModel` (`model.ts`) — our own neutral
// shape — so no chart library's types leak into the module, and replacing the
// engine is this one file. The colours are the alo tokens, read from the
// document rather than restated here, so a chart cannot drift from the design
// system it sits in.
import { useEffect, useRef } from "react";
import type { BarSeriesOption, LineSeriesOption, PieSeriesOption } from "echarts/charts";
import { BarChart, LineChart, PieChart } from "echarts/charts";
import type {
  GridComponentOption,
  LegendComponentOption,
  TooltipComponentOption,
} from "echarts/components";
import { GridComponent, LegendComponent, TooltipComponent } from "echarts/components";
import type { ComposeOption, EChartsType } from "echarts/core";
import { init, use } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";

import type { ChartModel } from "./model";
import styles from "./EChart.module.css";

use([BarChart, LineChart, PieChart, GridComponent, LegendComponent, TooltipComponent, CanvasRenderer]);

type Option = ComposeOption<
  | BarSeriesOption
  | LineSeriesOption
  | PieSeriesOption
  | GridComponentOption
  | LegendComponentOption
  | TooltipComponentOption
>;

/** What ECharts hands a tooltip formatter, narrowed to the three fields we
 *  read. Declared here rather than imported so the rest of the module keeps no
 *  opinion about the engine's callback shapes. */
interface TooltipParam {
  seriesIndex?: number;
  dataIndex?: number;
  name?: string;
  marker?: string;
}

/** An alo token, or the value to fall back to where the document has none
 *  (a test renderer, a detached node). */
function token(style: CSSStyleDeclaration, name: string, fallback: string): string {
  const value = style.getPropertyValue(name).trim();
  return value === "" ? fallback : value;
}

/** The series colours: the alo accents, in the order a reader meets them. Two
 *  currencies on one chart must be told apart at a glance, which is the whole
 *  job of this list. */
function palette(style: CSSStyleDeclaration): string[] {
  return [
    token(style, "--verdigris-500", "#e76f51"),
    token(style, "--navy-500", "#1f3d5b"),
    token(style, "--success", "#2e8b57"),
    token(style, "--warning", "#f59e0b"),
    token(style, "--copper-600", "#b84a32"),
    token(style, "--navy-400", "#35506b"),
    token(style, "--verdigris-200", "#f1bdad"),
    token(style, "--navy-100", "#c6d2de"),
  ];
}

/** Text that a tenant typed is never put into tooltip HTML unescaped: a
 *  customer called `<img onerror=…>` is a customer, not a script. */
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** The tooltip: the bucket, then each group's figure **as the server stated
 *  it** — the formatted text from the model, never a number this file rounds. */
function tooltip(model: ChartModel, params: TooltipParam[]): string {
  const first = params[0];
  if (first === undefined) return "";
  const head = escapeHtml(first.name ?? "");
  const rows = params.map((param) => {
    const series = model.series[param.seriesIndex ?? 0];
    const cell = series?.values[param.dataIndex ?? 0];
    const name = model.multi && series !== undefined ? `${escapeHtml(series.name)}: ` : "";
    return `<div>${param.marker ?? ""}${name}<b>${escapeHtml(cell?.text ?? "")}</b></div>`;
  });
  return `<div><b>${head}</b></div>${rows.join("")}`;
}

/** The option for a bar or line chart: one cartesian series per group. */
function cartesian(model: ChartModel, style: CSSStyleDeclaration): Option {
  const axis = token(style, "--text-tertiary", "#6b7280");
  const line = token(style, "--border-subtle", "#ece6dc");
  return {
    grid: { left: 8, right: 12, top: model.multi ? 30 : 12, bottom: 4, containLabel: true },
    // A legend earns its space only when there is more than one group to tell
    // apart — which, for money, is when the answer is in two currencies.
    ...(model.multi ? { legend: { top: 0, textStyle: { color: axis } } } : {}),
    xAxis: {
      type: "category",
      data: model.categories,
      axisLabel: { color: axis, hideOverlap: true },
      axisLine: { lineStyle: { color: line } },
      axisTick: { show: false },
    },
    yAxis: {
      type: "value",
      axisLabel: { color: axis, formatter: (value: number) => model.axisLabel(value) },
      splitLine: { lineStyle: { color: line } },
    },
    series:
      model.kind === "line"
        ? model.series.map(
            (series): LineSeriesOption => ({
              type: "line",
              name: series.name,
              data: series.values.map((v) => v.value),
              smooth: false,
              // Dots stop being informative long before a five-year daily
              // series is drawn; the line still is.
              showSymbol: model.categories.length <= 40,
            }),
          )
        : model.series.map(
            (series): BarSeriesOption => ({
              type: "bar",
              name: series.name,
              data: series.values.map((v) => v.value),
              barMaxWidth: 42,
              itemStyle: { borderRadius: [3, 3, 0, 0] },
            }),
          ),
  };
}

/** The option for a pie: one whole, sliced. The caller has already narrowed the
 *  model to a single group, because shares of a whole are shares of one whole. */
function pie(model: ChartModel, style: CSSStyleDeclaration): Option {
  const text = token(style, "--text-secondary", "#334155");
  const group = model.series[0];
  return {
    legend: { type: "scroll", bottom: 0, textStyle: { color: text } },
    series: [
      {
        type: "pie",
        radius: ["42%", "68%"],
        center: ["50%", "44%"],
        avoidLabelOverlap: true,
        label: { show: false },
        data: model.categories.map((name, index) => ({
          name,
          value: group?.values[index]?.value ?? 0,
        })),
      },
    ],
  };
}

/**
 * One chart, drawn into a canvas that fills its tile.
 *
 * The instance is created once per mount and re-optioned when the model
 * changes (`notMerge`, so a chart that lost a series does not keep drawing it),
 * resized with its container, and disposed on unmount — a chart engine that
 * leaks instances is a workspace that gets slower the longer it is open.
 */
export function EChart({ model, label }: { model: ChartModel; label: string }) {
  const host = useRef<HTMLDivElement>(null);
  const chart = useRef<EChartsType | null>(null);

  useEffect(() => {
    const node = host.current;
    if (node === null) return;
    const instance = init(node, undefined, { renderer: "canvas" });
    chart.current = instance;
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => instance.resize());
    observer?.observe(node);
    return () => {
      observer?.disconnect();
      chart.current = null;
      instance.dispose();
    };
  }, []);

  useEffect(() => {
    const instance = chart.current;
    if (instance === null) return;
    const style = getComputedStyle(document.documentElement);
    const base = model.kind === "pie" ? pie(model, style) : cartesian(model, style);
    instance.setOption(
      {
        ...base,
        color: palette(style),
        textStyle: { fontFamily: token(style, "--font-ui", "Inter, system-ui, sans-serif") },
        animationDuration: 240,
        tooltip: {
          trigger: model.kind === "pie" ? "item" : "axis",
          confine: true,
          formatter: (params: TooltipParam | TooltipParam[]) =>
            tooltip(model, Array.isArray(params) ? params : [params]),
        },
      },
      { notMerge: true },
    );
  }, [model]);

  return <div ref={host} className={styles.host} role="img" aria-label={label} />;
}
