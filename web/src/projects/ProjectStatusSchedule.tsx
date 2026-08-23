import { CalendarDays, Check, CircleDashed } from "lucide-react";

import { DatePicker } from "../ds";
import { strings } from "../i18n";
import type { Project } from "./types";

const statuses: Project["status"][] = [
  "planned",
  "active",
  "on_hold",
  "completed",
  "cancelled",
];

function statusLabel(status: Project["status"]): string {
  return {
    planned: strings.projectsStatusPlanned,
    active: strings.projectsStatusActive,
    on_hold: strings.projectsStatusOnHold,
    completed: strings.projectsStatusCompleted,
    cancelled: strings.projectsStatusCancelled,
  }[status];
}

export function ProjectStatusSchedule({
  status,
  startsOn,
  targetOn,
  datesValid,
  onStatusChange,
  onStartsOnChange,
  onTargetOnChange,
}: {
  status: Project["status"];
  startsOn: string;
  targetOn: string;
  datesValid: boolean;
  onStatusChange: (status: Project["status"]) => void;
  onStartsOnChange: (value: string) => void;
  onTargetOnChange: (value: string) => void;
}) {
  return (
    <section className="space-y-4 rounded-xl border border-subtle bg-raised/60 p-4">
      <fieldset className="m-0 border-0 p-0">
        <legend className="mb-2.5 text-sm font-semibold text-primary">
          {strings.projectsStatus}
        </legend>
        <div
          className="grid grid-cols-2 gap-1 rounded-lg bg-surface p-1 sm:grid-cols-5"
          role="group"
          aria-label={strings.projectsStatus}
        >
          {statuses.map((option) => {
            const selected = status === option;
            return (
              <button
                key={option}
                type="button"
                aria-pressed={selected}
                className={`flex min-h-10 items-center justify-center gap-1.5 rounded-md px-3 py-2 text-sm font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
                  selected
                    ? "bg-accent text-on-accent shadow-sm"
                    : "text-secondary hover:bg-raised hover:text-primary"
                }`}
                onClick={() => onStatusChange(option)}
              >
                {selected ? (
                  <Check className="size-3.5 shrink-0" aria-hidden="true" />
                ) : (
                  <CircleDashed
                    className="size-3.5 shrink-0 opacity-50"
                    aria-hidden="true"
                  />
                )}
                <span className="whitespace-nowrap">{statusLabel(option)}</span>
              </button>
            );
          })}
        </div>
      </fieldset>

      <div className="border-t border-subtle pt-4">
        <div className="mb-2.5 flex items-center gap-2 text-sm font-semibold text-primary">
          <CalendarDays className="size-4 text-accent" aria-hidden="true" />
          <span>{strings.projectsTabPlan}</span>
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="flex min-w-0 flex-col gap-1.5">
            <span className="text-xs font-medium text-secondary">
              {strings.projectsStartsOn}
            </span>
            <DatePicker
              value={startsOn}
              onChange={onStartsOnChange}
              placeholder={strings.projectsStartsOn}
            />
          </label>
          <label className="flex min-w-0 flex-col gap-1.5">
            <span className="text-xs font-medium text-secondary">
              {strings.projectsTargetOn}
            </span>
            <DatePicker
              value={targetOn}
              onChange={onTargetOnChange}
              placeholder={strings.projectsTargetOn}
              aria-invalid={!datesValid}
            />
            {!datesValid && (
              <span className="text-xs text-danger">
                {strings.projectsDatesInvalid}
              </span>
            )}
          </label>
        </div>
      </div>
    </section>
  );
}
