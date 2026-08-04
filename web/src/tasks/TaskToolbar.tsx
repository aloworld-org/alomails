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
import styles from "./TasksModule.module.css";

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
    <div className={styles.tbWrap} ref={ref}>
      <button
        type="button"
        className={`${styles.tbBtn} ${active === true ? styles.tbBtnActive : ""}`}
        onClick={() => setOpen((v) => !v)}
      >
        {icon}
        {label}
        {active === true && <span className={styles.tbDot} aria-hidden />}
      </button>
      {open && (
        <div className={styles.tbMenu} role="menu">
          {children}
        </div>
      )}
    </div>
  );
}

function Choice({ on, label, onClick }: { on: boolean; label: string; onClick: () => void }) {
  return (
    <button type="button" role="menuitemradio" aria-checked={on} className={styles.tbItem} onClick={onClick}>
      <span className={styles.tbCheck}>{on && <Check size={14} />}</span>
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
    <div className={styles.toolbar2}>
      <Dropdown label={strings.taskFilter} icon={<ListFilter size={15} />} active={isFiltering(config)}>
        <div className={styles.tbGroupLabel}>{strings.taskPriority}</div>
        {prios.map((p) => (
          <button
            key={p.key}
            type="button"
            role="menuitemcheckbox"
            aria-checked={config.priorities.has(p.key)}
            className={styles.tbItem}
            onClick={() => togglePriority(p.key)}
          >
            <span className={styles.tbCheck}>{config.priorities.has(p.key) && <Check size={14} />}</span>
            {p.label}
          </button>
        ))}
        <div className={styles.tbSep} />
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
