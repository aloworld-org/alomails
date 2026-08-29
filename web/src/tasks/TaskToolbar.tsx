// The List toolbar: real Filter / Sort / Group / Options controls that reshape
// the loaded tasks (see viewConfig). Each is a small popover that closes on an
// outside click or Escape. No control invents data — they only hide, order, or
// bucket what the API returned.
import { useCallback, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Check, ListFilter, ArrowUpDown, Rows3, SlidersHorizontal } from "lucide-react";

import { useDismiss } from "../ds";
import { strings } from "../i18n";
import type { TaskPriority } from "../jmap";
import { isFiltering, type GroupKey, type SortKey, type ViewConfig } from "./viewConfig";

interface Props {
  config: ViewConfig;
  onChange: (next: ViewConfig) => void;
  /** Optional list metrics share the same command surface as the controls. */
  summary?: ReactNode;
}

/** A view-config dropdown: stays open across choices, holds `menuitemradio`/
 *  `menuitemcheckbox` items and a section heading. Read against `ds/Menu` and
 *  `ds/ChoicePicker` before staying local (D2.11b): `Menu` is a menu of
 *  *actions* — it closes on every choice and its items carry no checked state —
 *  and `ChoicePicker` is a form field holding one value, drawn as a combobox.
 *  Neither says "reshape the list and keep adjusting"; forcing either would
 *  have meant growing it a second personality. The dismissal, the one piece of
 *  behaviour every popover must share, comes from `ds/useDismiss`. */
function Dropdown({
  label,
  icon,
  active,
  children,
}: {
  label: string;
  icon: ReactNode;
  active?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, ref, close);
  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        className={`relative inline-flex rounded-xl border border-subtle bg-app text-sm font-medium !no-underline shadow-sm transition-colors hover:border-default hover:bg-raised hover:text-primary hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${active === true ? "border-accent/40 text-accent" : "text-secondary"}`}
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <span className="inline-flex min-h-10 items-center gap-2 px-4 py-2">
          {icon}
          {label}
        </span>
        {active === true && <span className="absolute right-1 top-1 size-1.5 rounded-full bg-accent" aria-hidden />}
      </button>
      {open && (
        <div className="absolute left-0 top-[calc(100%+0.25rem)] z-[var(--z-overlay)] flex min-w-48 flex-col gap-px rounded-lg border border-default bg-surface p-1.5 shadow-lg" role="menu">
          {children}
        </div>
      )}
    </div>
  );
}

function Choice({ on, label, onClick }: { on: boolean; label: string; onClick: () => void }) {
  return (
    <button type="button" role="menuitemradio" aria-checked={on} className="flex min-h-9 items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-primary !no-underline transition-colors hover:!bg-accent-soft hover:!text-accent hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent" onClick={onClick}>
      <span className="inline-flex w-4 shrink-0 justify-center text-accent">{on && <Check size={14} />}</span>
      {label}
    </button>
  );
}

export function TaskToolbar({ config, onChange, summary }: Props) {
  const set = (patch: Partial<ViewConfig>) => onChange({ ...config, ...patch });

  const sortItems: { key: SortKey; label: string }[] = [
    { key: "manual", label: strings.taskSortManual },
    { key: "due", label: strings.taskSortDue },
    { key: "priority", label: strings.taskSortPriority },
    { key: "name", label: strings.taskSortName },
    { key: "created", label: strings.taskSortCreated },
  ];
  const groupItems: { key: GroupKey; label: string }[] = [
    { key: "status", label: strings.taskGroupStatus },
    { key: "project", label: strings.taskGroupProject },
    { key: "assignee", label: strings.taskGroupAssignee },
    { key: "priority", label: strings.taskGroupPriority },
    { key: "none", label: strings.taskGroupNone },
  ];
  const prios: { key: TaskPriority; label: string }[] = [
    { key: "high", label: strings.taskPrioHigh },
    { key: "medium", label: strings.taskPrioMedium },
    { key: "low", label: strings.taskPrioLow },
    { key: "none", label: strings.taskPrioNone },
  ];

  function togglePriority(p: TaskPriority) {
    const next = new Set(config.priorities);
    if (next.has(p)) next.delete(p);
    else next.add(p);
    set({ priorities: next });
  }

  return (
    <div className="mx-auto mt-4 flex w-[calc(100%-3rem)] max-w-[97rem] flex-wrap items-center gap-2 rounded-2xl border border-subtle bg-surface p-2 shadow-sm max-sm:w-[calc(100%-2rem)]">
      <Dropdown label={strings.taskFilter} icon={<ListFilter size={15} />} active={isFiltering(config)}>
        <div className="px-2 pb-0.5 pt-1.5 text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.taskPriority}</div>
        {prios.map((p) => (
          <button
            key={p.key}
            type="button"
            role="menuitemcheckbox"
            aria-checked={config.priorities.has(p.key)}
            className="flex min-h-9 items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-primary !no-underline transition-colors hover:!bg-accent-soft hover:!text-accent hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent"
            onClick={() => togglePriority(p.key)}
          >
            <span className="inline-flex w-4 shrink-0 justify-center text-accent">{config.priorities.has(p.key) && <Check size={14} />}</span>
            {p.label}
          </button>
        ))}
        <div className="my-1 h-px bg-subtle" />
        <Choice on={config.onlyMine} label={strings.taskOnlyMine} onClick={() => set({ onlyMine: !config.onlyMine })} />
      </Dropdown>

      <Dropdown label={strings.taskSort} icon={<ArrowUpDown size={15} />}>
        {sortItems.map((s) => (
          <Choice key={s.key} on={config.sort === s.key} label={s.label} onClick={() => set({ sort: s.key })} />
        ))}
      </Dropdown>

      <Dropdown label={strings.taskGroup} icon={<Rows3 size={15} />}>
        {groupItems.map((g) => (
          <Choice key={g.key} on={config.group === g.key} label={g.label} onClick={() => set({ group: g.key })} />
        ))}
      </Dropdown>

      <Dropdown label={strings.taskOptions} icon={<SlidersHorizontal size={15} />}>
        <Choice
          on={config.showCompleted}
          label={strings.taskShowCompleted}
          onClick={() => set({ showCompleted: !config.showCompleted })}
        />
        <Choice
          on={config.compact}
          label={strings.taskCompactRows}
          onClick={() => set({ compact: !config.compact })}
        />
      </Dropdown>
      {summary !== undefined && (
        <div className="ml-auto flex flex-wrap items-center gap-1 border-l border-subtle pl-3 max-lg:ml-0 max-lg:w-full max-lg:border-l-0 max-lg:border-t max-lg:pl-0 max-lg:pt-2">
          {summary}
        </div>
      )}
    </div>
  );
}
