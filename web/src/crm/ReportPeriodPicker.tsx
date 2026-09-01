import { useEffect, useMemo, useRef, useState } from "react";
import { CalendarRange, ChevronDown } from "lucide-react";

import { previousQuarterOf, quarterOf, type Period } from "../billing";
import { Button, DatePicker, Field } from "../ds";
import { getLocale, strings } from "../i18n";

function ymd(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

function lastDays(count: number): Period {
  const to = new Date();
  const from = new Date(to);
  from.setDate(to.getDate() - count + 1);
  return { from: ymd(from), to: ymd(to) };
}

function same(a: Period, b: Period): boolean {
  return a.from === b.from && a.to === b.to;
}

export function ReportPeriodPicker({ value, onApply }: { value: Period; onApply: (period: Period) => void }) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(value);
  const root = useRef<HTMLDivElement>(null);
  const today = useMemo(() => ymd(new Date()), []);
  const yesterday = useMemo(() => {
    const date = new Date();
    date.setDate(date.getDate() - 1);
    return ymd(date);
  }, []);
  const presets = useMemo(() => [
    { label: strings.crmReportToday, period: { from: today, to: today } },
    { label: strings.crmReportYesterday, period: { from: yesterday, to: yesterday } },
    { label: strings.crmReportLast7Days, period: lastDays(7) },
    { label: strings.crmReportLast28Days, period: lastDays(28) },
    { label: strings.crmReportLast30Days, period: lastDays(30) },
    { label: strings.crmReportThisQuarter, period: quarterOf(new Date()) },
    { label: strings.crmReportLastQuarter, period: previousQuarterOf(new Date()) },
  ], [today, yesterday]);

  useEffect(() => setDraft(value), [value]);
  useEffect(() => {
    if (!open) return undefined;
    function close(event: PointerEvent) {
      if (root.current !== null && !root.current.contains(event.target as Node)) setOpen(false);
    }
    function escape(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  const locale = getLocale();
  const format = (day: string) => new Intl.DateTimeFormat(locale, { day: "numeric", month: "short", year: "numeric" }).format(new Date(`${day}T00:00`));
  const activePreset = presets.find((preset) => same(preset.period, value));

  return (
    <div className="relative" ref={root}>
      <button type="button" className="flex min-h-11 min-w-[18rem] items-center gap-3 rounded-xl border border-default bg-surface px-4 text-left transition-colors hover:border-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/20 max-sm:min-w-0 max-sm:w-full" aria-haspopup="dialog" aria-expanded={open} onClick={() => { setDraft(value); setOpen((current) => !current); }}>
        <CalendarRange className="size-4 shrink-0 text-accent" aria-hidden="true" />
        <span className="min-w-0 flex-1">
          <span className="block text-xs font-medium text-secondary">{activePreset?.label ?? strings.crmReportCustom}</span>
          <span className="block truncate text-sm font-semibold text-primary">{format(value.from)} – {format(value.to)}</span>
        </span>
        <ChevronDown className="size-4 shrink-0 text-tertiary" aria-hidden="true" />
      </button>

      {open && (
        <div className="absolute left-0 top-[calc(100%+.5rem)] z-50 grid w-[42rem] grid-cols-[13rem_minmax(0,1fr)] overflow-visible rounded-2xl border border-subtle bg-surface shadow-xl max-md:w-[min(34rem,calc(100vw-2rem))] max-md:grid-cols-1" role="dialog" aria-label={strings.crmReportPeriod}>
          <div className="border-r border-subtle p-2 max-md:border-b max-md:border-r-0">
            <p className="m-0 px-3 py-2 text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.crmReportQuickRanges}</p>
            <div className="flex flex-col gap-1 max-md:grid max-md:grid-cols-2">
              {presets.map((preset) => (
                <button key={preset.label} type="button" className={`min-h-10 rounded-lg px-3 text-left text-sm font-medium transition-colors ${same(draft, preset.period) ? "bg-accent-soft text-accent" : "text-secondary hover:bg-raised hover:text-primary"}`} onClick={() => setDraft(preset.period)}>
                  {preset.label}
                </button>
              ))}
            </div>
          </div>
          <div className="p-5">
            <h3 className="m-0 text-base font-semibold text-primary">{strings.crmReportCustom}</h3>
            <div className="mt-4 grid grid-cols-2 gap-3 max-sm:grid-cols-1">
              <Field label={strings.crmReportFrom}>{(control) => <DatePicker {...control} value={draft.from} onChange={(from) => setDraft({ ...draft, from })} />}</Field>
              <Field label={strings.crmReportTo}>{(control) => <DatePicker {...control} value={draft.to} onChange={(to) => setDraft({ ...draft, to })} />}</Field>
            </div>
            <div className="mt-6 flex justify-end gap-2 border-t border-subtle pt-4">
              <Button variant="ghost" onClick={() => { setDraft(value); setOpen(false); }}>{strings.crmCancel}</Button>
              <Button disabled={draft.from === "" || draft.to === "" || draft.from > draft.to} onClick={() => { onApply(draft); setOpen(false); }}>{strings.crmReportApply}</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
