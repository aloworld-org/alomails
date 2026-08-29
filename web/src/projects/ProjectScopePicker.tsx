import { useEffect, useId, useRef, useState } from "react";
import { Check, ChevronDown, FolderKanban, Layers3 } from "lucide-react";

import { strings } from "../i18n";
import type { Project } from "./types";

interface Props {
  projects: Pick<Project, "id" | "name">[];
  value: string | null;
  onChange: (projectId: string | null) => void;
  description?: string;
  disabled?: boolean;
  /** Keeps the picker inside a screen toolbar instead of making it a card. */
  compact?: boolean;
}

/**
 * One scope control for every portfolio view. The URL remains the source of
 * truth; this component only presents the current scope and reports a choice.
 * Keeping the same control in Timesheet, Timeline, and Reports prevents a
 * selected project from feeling like three unrelated filters.
 */
export function ProjectScopePicker({
  projects,
  value,
  onChange,
  description,
  disabled = false,
  compact = false,
}: Props) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const listboxId = useId();
  const selected = projects.find((project) => project.id === value) ?? null;
  const selectedLabel = selected?.name ?? strings.projectsAllProjects;

  useEffect(() => {
    if (!open) return;
    function closeOnOutsidePointer(event: PointerEvent) {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    }
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  function choose(next: string | null) {
    onChange(next);
    setOpen(false);
  }

  return (
    <section
      className={
        compact
          ? "flex min-w-0 flex-wrap items-end gap-3"
          : "flex flex-wrap items-end gap-4 rounded-2xl border border-subtle bg-surface px-5 py-4 shadow-sm"
      }
    >
      <div ref={rootRef} className="relative min-w-64 flex-1 sm:max-w-sm">
        <span className="mb-1.5 block text-xs font-semibold uppercase tracking-wide text-tertiary">
          {strings.projectsProject}
        </span>
        <button
          type="button"
          className={`flex w-full items-center gap-3 rounded-xl border border-default bg-surface px-3.5 text-left text-primary transition-colors hover:bg-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:cursor-not-allowed disabled:opacity-60 ${compact ? "min-h-10 py-1.5" : "min-h-12 py-2"}`}
          aria-haspopup="listbox"
          aria-expanded={open}
          aria-controls={listboxId}
          disabled={disabled}
          onClick={() => setOpen((current) => !current)}
        >
          <span
            className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent"
            aria-hidden="true"
          >
            {selected === null ? (
              <Layers3 size={17} />
            ) : (
              <FolderKanban size={17} />
            )}
          </span>
          <span className="min-w-0 flex-1 truncate text-sm font-medium">
            {selectedLabel}
          </span>
          <ChevronDown
            className={`shrink-0 text-tertiary transition-transform ${open ? "rotate-180" : ""}`}
            size={17}
            aria-hidden="true"
          />
        </button>

        {open && (
          <div
            id={listboxId}
            role="listbox"
            aria-label={strings.projectsProject}
            className="absolute left-0 top-full z-40 mt-2 max-h-72 w-full overflow-auto rounded-xl border border-default bg-surface p-1.5 shadow-lg"
          >
            <ScopeOption
              label={strings.projectsAllProjects}
              selected={value === null}
              onChoose={() => choose(null)}
              portfolio
            />
            {projects.map((project) => (
              <ScopeOption
                key={project.id}
                label={project.name}
                selected={project.id === value}
                onChoose={() => choose(project.id)}
              />
            ))}
          </div>
        )}
      </div>
      {description && (
        <p className="m-0 max-w-xl pb-1 text-sm leading-6 text-secondary">
          {description}
        </p>
      )}
    </section>
  );
}

function ScopeOption({
  label,
  selected,
  onChoose,
  portfolio = false,
}: {
  label: string;
  selected: boolean;
  onChoose: () => void;
  portfolio?: boolean;
}) {
  return (
    <button
      type="button"
      role="option"
      aria-selected={selected}
      className={`flex min-h-10 w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition-colors hover:!bg-accent-soft hover:!text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${selected ? "!bg-accent-soft font-medium !text-accent" : "text-primary"}`}
      onClick={onChoose}
    >
      {portfolio ? (
        <Layers3 size={16} aria-hidden="true" />
      ) : (
        <FolderKanban size={16} aria-hidden="true" />
      )}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {selected && <Check size={16} aria-hidden="true" />}
    </button>
  );
}
