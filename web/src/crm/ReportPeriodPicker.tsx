import { useEffect, useMemo, useRef, useState } from "react";
import { CalendarRange, ChevronDown, ChevronLeft, ChevronRight } from "lucide-react";

import { previousQuarterOf, quarterOf, type Period } from "../billing";
import { Button } from "../ds";
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
  const [choosing, setChoosing] = useState<"from" | "to">("from");
  const [anchor, setAnchor] = useState(() => new Date(`${value.from}T00:00`));
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
      <button type="button" className="flex min-h-11 min-w-[18rem] items-center gap-3 rounded-xl border border-default bg-surface px-4 text-left transition-colors hover:border-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/20 max-sm:min-w-0 max-sm:w-full" aria-haspopup="dialog" aria-expanded={open} onClick={() => { setDraft(value); setAnchor(new Date(`${value.from}T00:00`)); setChoosing("from"); setOpen((current) => !current); }}>
        <CalendarRange className="size-4 shrink-0 text-accent" aria-hidden="true" />
        <span className="min-w-0 flex-1">
          <span className="block text-xs font-medium text-secondary">{activePreset?.label ?? strings.crmReportCustom}</span>
          <span className="block truncate text-sm font-semibold text-primary">{format(value.from)} – {format(value.to)}</span>
        </span>
        <ChevronDown className="size-4 shrink-0 text-tertiary" aria-hidden="true" />
      </button>

      {open && (
        <div className="absolute left-0 top-[calc(100%+.5rem)] z-50 grid w-[58rem] grid-cols-[13rem_minmax(0,1fr)] overflow-hidden rounded-2xl border border-subtle bg-surface shadow-xl max-xl:w-[min(48rem,calc(100vw-4rem))] max-md:w-[min(34rem,calc(100vw-2rem))] max-md:grid-cols-1" role="dialog" aria-label={strings.crmReportPeriod}>
          <div className="border-r border-subtle p-2 max-md:border-b max-md:border-r-0">
            <p className="m-0 px-3 py-2 text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.crmReportQuickRanges}</p>
            <div className="flex flex-col gap-1 max-md:grid max-md:grid-cols-2">
              {presets.map((preset) => (
                <button key={preset.label} type="button" className={`min-h-10 rounded-lg px-3 text-left text-sm font-medium transition-colors ${same(draft, preset.period) ? "bg-accent-soft text-accent" : "text-secondary hover:bg-raised hover:text-primary"}`} onClick={() => { setDraft(preset.period); setAnchor(new Date(`${preset.period.from}T00:00`)); setChoosing("from"); }}>
                  {preset.label}
                </button>
              ))}
            </div>
          </div>
          <div className="p-5">
            <h3 className="m-0 text-base font-semibold text-primary">{strings.crmReportCustom}</h3>
            <div className="mt-4 grid grid-cols-2 gap-3">
              <button type="button" className={`rounded-xl border p-3 text-left transition-colors ${choosing === "from" ? "border-accent bg-accent-soft" : "border-default bg-surface hover:border-strong"}`} onClick={() => setChoosing("from")}>
                <span className="block text-xs font-medium text-secondary">{strings.crmReportFrom}</span>
                <span className="mt-1 block text-sm font-semibold text-primary">{format(draft.from)}</span>
              </button>
              <button type="button" className={`rounded-xl border p-3 text-left transition-colors ${choosing === "to" ? "border-accent bg-accent-soft" : "border-default bg-surface hover:border-strong"}`} onClick={() => setChoosing("to")}>
                <span className="block text-xs font-medium text-secondary">{strings.crmReportTo}</span>
                <span className="mt-1 block text-sm font-semibold text-primary">{format(draft.to)}</span>
              </button>
            </div>
            <RangeCalendar anchor={anchor} period={draft} locale={locale} onPrevious={() => setAnchor(new Date(anchor.getFullYear(), anchor.getMonth() - 1, 1))} onNext={() => setAnchor(new Date(anchor.getFullYear(), anchor.getMonth() + 1, 1))} onPick={(day) => {
              if (choosing === "from") {
                setDraft({ from: day, to: day > draft.to ? day : draft.to });
                setChoosing("to");
              } else {
                setDraft(day < draft.from ? { from: day, to: draft.from } : { ...draft, to: day });
                setChoosing("from");
              }
            }} />
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

function monthDays(month: Date): Array<Date | null> {
  const first = new Date(month.getFullYear(), month.getMonth(), 1);
  const leading = (first.getDay() + 6) % 7;
  const count = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate();
  return [...Array<Date | null>(leading).fill(null), ...Array.from({ length: count }, (_, index) => new Date(month.getFullYear(), month.getMonth(), index + 1))];
}

function RangeCalendar({ anchor, period, locale, onPrevious, onNext, onPick }: { anchor: Date; period: Period; locale: string; onPrevious: () => void; onNext: () => void; onPick: (day: string) => void }) {
  const end = new Date(`${period.to}T00:00`);
  const endMonth = new Date(end.getFullYear(), end.getMonth(), 1);
  const startMonth = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
  const secondMonth = endMonth.getTime() > startMonth.getTime() ? endMonth : new Date(anchor.getFullYear(), anchor.getMonth() + 1, 1);
  const weekday = new Intl.DateTimeFormat(locale, { weekday: "narrow" });
  const weekdays = Array.from({ length: 7 }, (_, index) => weekday.format(new Date(2024, 0, 1 + index)));
  return (
    <div className="mt-5 rounded-xl border border-subtle bg-raised/20 p-3">
      <div className="mb-3 flex items-center justify-between">
        <button type="button" className="grid size-10 place-items-center rounded-lg text-secondary hover:bg-raised hover:text-primary" onClick={onPrevious} aria-label={strings.agendaPrev}><ChevronLeft size={18} /></button>
        <p className="m-0 text-xs font-medium text-secondary">{formatRange(period, locale)}</p>
        <button type="button" className="grid size-10 place-items-center rounded-lg text-secondary hover:bg-raised hover:text-primary" onClick={onNext} aria-label={strings.agendaNext}><ChevronRight size={18} /></button>
      </div>
      <div className="grid grid-cols-2 gap-5 max-lg:grid-cols-1">
        {[startMonth, secondMonth].map((month) => (
          <div key={`${month.getFullYear()}-${month.getMonth()}`}>
            <p className="mb-3 mt-0 text-center text-sm font-semibold capitalize text-primary">{new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" }).format(month)}</p>
            <div className="grid grid-cols-7">
              {weekdays.map((day, index) => <span key={index} className="grid h-8 place-items-center text-[0.65rem] font-semibold uppercase text-tertiary">{day}</span>)}
              {monthDays(month).map((date, index) => {
                if (date === null) return <span key={`empty-${index}`} />;
                const day = ymd(date);
                const endpoint = day === period.from || day === period.to;
                const inRange = day > period.from && day < period.to;
                return <button key={day} type="button" aria-pressed={endpoint} aria-label={new Intl.DateTimeFormat(locale, { dateStyle: "long" }).format(date)} className={`grid aspect-square min-h-9 place-items-center text-sm tabular-nums transition-colors ${endpoint ? "rounded-lg bg-accent font-semibold text-on-accent shadow-sm" : inRange ? "rounded-none bg-accent-soft text-primary hover:bg-accent-soft" : "rounded-lg text-primary hover:bg-raised"}`} onClick={() => onPick(day)}>{date.getDate()}</button>;
              })}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function formatRange(period: Period, locale: string): string {
  const format = new Intl.DateTimeFormat(locale, { day: "numeric", month: "short", year: "numeric" });
  return `${format.format(new Date(`${period.from}T00:00`))} – ${format.format(new Date(`${period.to}T00:00`))}`;
}
