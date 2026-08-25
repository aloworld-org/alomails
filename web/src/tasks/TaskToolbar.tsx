// The List toolbar: real Filter / Sort / Group / Options controls that reshape
// the loaded tasks (see viewConfig). Each is a small popover that closes on an
// outside click or Escape. No control invents data — they only hide, order, or
// bucket what the API returned.
import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Check, ListFilter, ArrowUpDown, Rows3, SlidersHorizontal } from "lucide-react";

import { strings } from "../i18n";
import type { TaskPriority } from "../jmap";
import { isFiltering, type GroupKey, type SortKey, type ViewConfig } from "./viewConfig";

interface Props {
  config: ViewConfig;
  onChange: (next: ViewConfig) => void;
}

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
  useEffect(() => {
    if (!open) return undefined;
    function down(e: PointerEvent) {
      if (ref.current !== null && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    function key(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", down);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", down);
      document.removeEventListener("keydown", key);
    };
  }, [open]);
  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        className={`relative inline-flex min-h-10 items-center gap-2 rounded-xl border border-subtle bg-app px-4 py-2 text-sm font-medium !no-underline shadow-sm transition-colors hover:border-default hover:bg-raised hover:text-primary hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${active === true ? "border-accent/40 text-accent" : "text-secondary"}`}
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        {icon}
        {label}
        {active === true && <span className="absolute right-1 top-1 size-1.5 rounded-full bg-accent" aria-hidden />}
      </button>
      {open && (
        <div className="absolute left-0 top-[calc(100%+0.25rem)] z-40 flex min-w-48 flex-col gap-px rounded-lg border border-default bg-surface p-1.5 shadow-lg" role="menu">
          {children}
        </div>
      )}
    </div>
  );
}

function Choice({ on, label, onClick }: { on: boolean; label: string; onClick: () => void }) {
  return (
    <button type="button" role="menuitemradio" aria-checked={on} className="flex min-h-9 items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-primary !no-underline hover:bg-raised hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent" onClick={onClick}>
      <span className="inline-flex w-4 shrink-0 justify-center text-accent">{on && <Check size={14} />}</span>
      {label}
    </button>
  );
}

export function TaskToolbar({ config, onChange }: Props) {
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
    <div className="mx-auto mt-4 flex w-[calc(100%-3rem)] max-w-[97rem] items-center gap-3 overflow-x-auto rounded-2xl border border-subtle bg-surface p-3 shadow-sm max-sm:w-[calc(100%-2rem)]">
      <Dropdown label={strings.taskFilter} icon={<ListFilter size={15} />} active={isFiltering(config)}>
        <div className="px-2 pb-0.5 pt-1.5 text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.taskPriority}</div>
        {prios.map((p) => (
          <button
            key={p.key}
            type="button"
            role="menuitemcheckbox"
            aria-checked={config.priorities.has(p.key)}
            className="flex min-h-9 items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-primary !no-underline hover:bg-raised hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent"
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
    </div>
  );
}
