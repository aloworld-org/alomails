import { useEffect, useState } from "react";
import {
  ArrowDownRight,
  ArrowUpRight,
  CalendarRange,
  CircleAlert,
  TrendingUp,
} from "lucide-react";

import { Button, Card, Select, Spinner } from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { amountLabel, dayLabel, today } from "./format";
import { ErrorBanner } from "./parts";
import type { CashForecast } from "./types";

type Horizon = 30 | 60 | 90;

export function CashFlowView() {
  const api = useFinanceApi();
  const [horizon, setHorizon] = useState<Horizon>(30);
  const [receivableDelay, setReceivableDelay] = useState(0);
  const [payableDelay, setPayableDelay] = useState(0);
  const [forecast, setForecast] = useState<CashForecast | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void api
      .cashForecast({ on: today(), horizon, receivableDelay, payableDelay })
      .then((answer) => {
        if (live) {
          setForecast(answer);
          setError(null);
        }
      })
      .catch((reason: unknown) => {
        if (live)
          setError(financeMessage(reason, strings.financeForecastLoadFailed));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [api, horizon, payableDelay, receivableDelay]);

  const ending = forecast?.points.at(-1)?.projectedBalanceCents ?? null;

  return (
    <main className="min-h-0 flex-1 overflow-auto px-8 py-6 max-sm:px-4">
      <div className="mx-auto flex w-full max-w-[108rem] flex-col gap-5">
        <section className="flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="m-0 text-xs font-semibold uppercase tracking-[0.12em] text-accent">
              {strings.financeForecastEyebrow}
            </p>
            <h2 className="m-0 mt-1 text-xl font-semibold tracking-tight text-primary">
              {strings.financeForecastTitle}
            </h2>
            <p className="m-0 mt-1 text-sm text-secondary">
              {strings.financeForecastSubtitle}
            </p>
          </div>
          <div
            className="flex flex-wrap items-center gap-1 rounded-xl bg-raised p-1"
            role="group"
            aria-label={strings.financeForecastTitle}
          >
            <Button
              variant={
                receivableDelay === 0 && payableDelay === 0
                  ? "primary"
                  : "ghost"
              }
              size="sm"
              onClick={() => {
                setReceivableDelay(0);
                setPayableDelay(0);
              }}
            >
              {strings.financeForecastExpected}
            </Button>
            <Button
              variant={
                receivableDelay === 14 && payableDelay === 0
                  ? "primary"
                  : "ghost"
              }
              size="sm"
              onClick={() => {
                setReceivableDelay(14);
                setPayableDelay(0);
              }}
            >
              {strings.financeForecastConservative}
            </Button>
            <Button
              variant={
                receivableDelay === -7 && payableDelay === 7
                  ? "primary"
                  : "ghost"
              }
              size="sm"
              onClick={() => {
                setReceivableDelay(-7);
                setPayableDelay(7);
              }}
            >
              {strings.financeForecastOptimistic}
            </Button>
          </div>
        </section>

        <Card pad="sm" className="flex flex-wrap items-end gap-4 bg-surface/95">
          <label className="flex min-w-44 flex-col gap-1.5 text-xs font-semibold text-secondary">
            {strings.financeForecastHorizon}
            <Select
              value={String(horizon)}
              onChange={(event) =>
                setHorizon(Number(event.target.value) as Horizon)
              }
            >
              <option value="30">{strings.financeForecast30Days}</option>
              <option value="60">{strings.financeForecast60Days}</option>
              <option value="90">{strings.financeForecast90Days}</option>
            </Select>
          </label>
          <label className="flex min-w-44 flex-col gap-1.5 text-xs font-semibold text-secondary">
            {strings.financeForecastCustomerDelay}
            <Select
              value={String(receivableDelay)}
              onChange={(event) =>
                setReceivableDelay(Number(event.target.value))
              }
            >
              {[-7, 0, 7, 14, 30].map((days) => (
                <option key={days} value={days}>
                  {strings.financeForecastDays(days)}
                </option>
              ))}
            </Select>
          </label>
          <label className="flex min-w-44 flex-col gap-1.5 text-xs font-semibold text-secondary">
            {strings.financeForecastSupplierDelay}
            <Select
              value={String(payableDelay)}
              onChange={(event) => setPayableDelay(Number(event.target.value))}
            >
              {[-7, 0, 7, 14, 30].map((days) => (
                <option key={days} value={days}>
                  {strings.financeForecastDays(days)}
                </option>
              ))}
            </Select>
          </label>
          {loading && <Spinner size={16} />}
        </Card>
        {error !== null && <ErrorBanner message={error} />}
        {forecast !== null && (
          <>
            {(forecast.unconvertedReceivables > 0 ||
              forecast.unconvertedPayables > 0) && (
              <div className="flex items-start gap-2 rounded-xl border border-warning/20 bg-warning-soft p-3 text-sm text-secondary">
                <CircleAlert className="mt-0.5 size-4 shrink-0 text-warning" />
                <span>
                  {strings.financeForecastUnconverted(
                    forecast.unconvertedReceivables +
                      forecast.unconvertedPayables,
                  )}
                </span>
              </div>
            )}
            <section className="grid gap-3 md:grid-cols-3">
              <Summary
                Icon={CalendarRange}
                label={strings.financeForecastPeriod}
                value={`${dayLabel(forecast.on)} – ${dayLabel(forecast.through)}`}
              />
              <Summary
                Icon={TrendingUp}
                label={strings.financeForecastOpening}
                value={
                  forecast.openingBalanceCents === null
                    ? strings.financeForecastUnavailable
                    : amountLabel(
                        forecast.openingBalanceCents,
                        forecast.currency,
                      )
                }
              />
              <Summary
                Icon={TrendingUp}
                label={strings.financeForecastProjected}
                value={
                  ending === null
                    ? strings.financeForecastUnavailable
                    : amountLabel(ending, forecast.currency)
                }
                accent={ending !== null && ending < 0}
              />
            </section>
            <Card as="section" pad="none" className="overflow-hidden">
              <div className="border-b border-subtle px-5 py-4">
                <h3 className="m-0 text-base font-semibold text-primary">
                  {strings.financeForecastWeekly}
                </h3>
                <p className="m-0 mt-1 text-sm text-secondary">
                  {strings.financeForecastWeeklyHint}
                </p>
              </div>
              <div className="overflow-x-auto">
                <table className="w-full border-collapse text-sm">
                  <thead>
                    <tr className="text-left text-xs font-semibold uppercase tracking-wide text-secondary">
                      <th className="px-5 py-3">
                        {strings.financeForecastWeek}
                      </th>
                      <th className="px-5 py-3 text-right">
                        {strings.financeForecastIncoming}
                      </th>
                      <th className="px-5 py-3 text-right">
                        {strings.financeForecastOutgoing}
                      </th>
                      <th className="px-5 py-3 text-right">
                        {strings.financeForecastNet}
                      </th>
                      <th className="px-5 py-3 text-right">
                        {strings.financeForecastBalance}
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-subtle">
                    {forecast.points.map((point) => (
                      <tr key={point.from} className="hover:bg-raised">
                        <td className="whitespace-nowrap px-5 py-3 font-medium text-primary">
                          {dayLabel(point.from)} – {dayLabel(point.to)}
                        </td>
                        <td className="px-5 py-3 text-right text-success">
                          <span className="inline-flex items-center gap-1">
                            <ArrowUpRight className="size-4" />
                            {amountLabel(
                              point.incomingCents,
                              forecast.currency,
                            )}
                          </span>
                        </td>
                        <td className="px-5 py-3 text-right text-primary">
                          <span className="inline-flex items-center gap-1">
                            <ArrowDownRight className="size-4" />
                            {amountLabel(
                              point.outgoingCents,
                              forecast.currency,
                            )}
                          </span>
                        </td>
                        <td
                          className={`px-5 py-3 text-right font-semibold ${point.netCents < 0 ? "text-danger" : "text-primary"}`}
                        >
                          {amountLabel(point.netCents, forecast.currency)}
                        </td>
                        <td
                          className={`px-5 py-3 text-right font-semibold ${point.projectedBalanceCents !== null && point.projectedBalanceCents < 0 ? "text-danger" : "text-primary"}`}
                        >
                          {point.projectedBalanceCents === null
                            ? "—"
                            : amountLabel(
                                point.projectedBalanceCents,
                                forecast.currency,
                              )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Card>
          </>
        )}
      </div>
    </main>
  );
}

function Summary({
  Icon,
  label,
  value,
  accent = false,
}: {
  Icon: typeof TrendingUp;
  label: string;
  value: string;
  accent?: boolean;
}) {
  return (
    <Card pad="sm">
      <div className="flex items-center gap-3">
        <span className="grid size-10 place-items-center rounded-xl bg-[var(--accent-soft)] text-accent">
          <Icon className="size-5" />
        </span>
        <div className="min-w-0">
          <p className="m-0 text-xs font-medium text-secondary">{label}</p>
          <p
            className={`m-0 mt-1 truncate text-lg font-semibold ${accent ? "text-danger" : "text-primary"}`}
          >
            {value}
          </p>
        </div>
      </div>
    </Card>
  );
}
