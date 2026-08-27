import { useEffect, useRef, useState } from "react";
import { BriefcaseBusiness, Building2, Check, ChevronDown, UserRound } from "lucide-react";

import { strings } from "../i18n";
import { DialogFrame, Field } from "./parts";
import { ProjectStatusSchedule } from "./ProjectStatusSchedule";

export interface NewProjectDraft {
  name: string;
  customerId: string | null;
  description: string | null;
  status: ProjectStatus;
  startsOn: string | null;
  targetOn: string | null;
}

type ProjectStatus = "planned" | "active" | "on_hold" | "completed" | "cancelled";

function CustomerPicker({ customers, value, onChange }: {
  customers: Array<{ id: string; name: string }>;
  value: string;
  onChange: (customerId: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const selected = customers.find((customer) => customer.id === value) ?? null;

  useEffect(() => {
    function dismiss(event: MouseEvent) {
      if (root.current !== null && !root.current.contains(event.target as Node)) setOpen(false);
    }
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", dismiss);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("mousedown", dismiss);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, []);

  return (
    <div ref={root} className="relative">
      <button
        type="button"
        role="combobox"
        aria-label={strings.projectsCustomer}
        aria-expanded={open}
        aria-controls="new-project-customer-list"
        className={`min-h-12 w-full rounded-lg border bg-surface text-left text-sm transition-colors focus-visible:outline-2 focus-visible:outline-accent ${open ? "border-accent ring-1 ring-accent" : "border-default hover:border-strong"}`}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="flex min-h-12 w-full items-center justify-between gap-4 px-4 py-2.5">
          <span className={`min-w-0 truncate ${selected === null ? "text-tertiary" : "font-medium text-primary"}`}>
            {selected?.name ?? strings.projectsCustomerPick}
          </span>
          <ChevronDown className={`size-4 shrink-0 text-secondary transition-transform ${open ? "rotate-180" : ""}`} aria-hidden="true" />
        </span>
      </button>
      {open && (
        <div
          id="new-project-customer-list"
          role="listbox"
          aria-label={strings.projectsCustomer}
          className="absolute inset-x-0 top-full z-[var(--z-overlay)] mt-2 max-h-56 overflow-y-auto rounded-lg border border-subtle bg-surface p-1.5 shadow-lg"
        >
          {customers.length === 0 && (
            <p className="px-4 py-3 text-sm text-secondary">{strings.projectsNoCustomersAvailable}</p>
          )}
          {customers.map((customer) => {
            const active = customer.id === value;
            return (
              <button
                key={customer.id}
                type="button"
                role="option"
                aria-selected={active}
                className={`min-h-10 w-full rounded-md text-left text-sm font-medium transition-colors hover:!bg-accent-soft hover:!text-accent ${active ? "!bg-accent-soft !text-accent" : "text-primary"}`}
                onClick={() => {
                  onChange(customer.id);
                  setOpen(false);
                }}
              >
                <span className="flex min-h-10 w-full items-center justify-between gap-3 px-4 py-2.5">
                  <span className="truncate">{customer.name}</span>
                  {active && <Check className="size-4 shrink-0" aria-hidden="true" />}
                </span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function NewProjectDialog({ customers, onClose, onCreate }: {
  customers: Array<{ id: string; name: string }>;
  onClose: () => void;
  onCreate: (draft: NewProjectDraft) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"internal" | "client">(() => customers.length > 0 ? "client" : "internal");
  const [customerId, setCustomerId] = useState("");
  const [description, setDescription] = useState("");
  const [status, setStatus] = useState<ProjectStatus>("planned");
  const [startsOn, setStartsOn] = useState("");
  const [targetOn, setTargetOn] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const datesValid = startsOn === "" || targetOn === "" || targetOn >= startsOn;
  const canSubmit = name.trim() !== "" && datesValid && (kind === "internal" || customerId !== "");

  async function create() {
    setBusy(true);
    setError(null);
    try {
      await onCreate({
        name: name.trim(),
        customerId: kind === "client" ? customerId : null,
        description: description.trim() || null,
        status,
        startsOn: startsOn || null,
        targetOn: targetOn || null,
      });
    } catch {
      setError(strings.projectsCreateFailed);
      setBusy(false);
    }
  }

  return (
    <DialogFrame Icon={BriefcaseBusiness} title={strings.projectsNewTitle} subtitle={strings.projectsNewSubtitle} error={error} busy={busy} canSubmit={canSubmit} submitLabel={strings.projectsCreate} onClose={onClose} onSubmit={() => void create()}>
      <Field label={strings.projectsName}>
        <input autoFocus className="min-h-11 w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent" value={name} placeholder={strings.projectsNamePlaceholder} maxLength={120} onChange={(event) => setName(event.target.value)} />
      </Field>
      <fieldset className="m-0 border-0 p-0">
        <legend className="mb-2 text-sm font-medium text-secondary">{strings.projectsWorkType}</legend>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2" role="radiogroup">
          {(["client", "internal"] as const).map((option) => {
            const selected = kind === option;
            const Icon = option === "client" ? UserRound : Building2;
            return (
              <button
                key={option}
                type="button"
                className={`relative flex min-h-20 items-center gap-3 rounded-2xl border-2 px-5 py-4 text-left transition-[border-color,background-color,box-shadow] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${selected ? "border-accent bg-accent-soft shadow-sm" : "border-default bg-surface hover:border-strong hover:bg-raised hover:shadow-md"}`}
                role="radio"
                aria-checked={selected}
                onClick={() => setKind(option)}
              >
                <span className={`flex size-10 shrink-0 items-center justify-center rounded-lg ${selected ? "bg-accent text-on-accent" : "bg-raised text-secondary"}`}>
                  <Icon className="size-5" aria-hidden="true" />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-semibold text-primary">{option === "client" ? strings.projectsClientWork : strings.projectsInternalWork}</span>
                  <span className="mt-0.5 block text-xs font-normal leading-5 text-secondary">{option === "client" ? strings.projectsClientWorkHint : strings.projectsInternalWorkHint}</span>
                </span>
                <span className={`flex size-5 shrink-0 items-center justify-center rounded-full border ${selected ? "border-accent bg-accent text-on-accent" : "border-strong bg-surface"}`}>
                  {selected && <Check className="size-3.5" aria-hidden="true" />}
                </span>
              </button>
            );
          })}
        </div>
      </fieldset>
      {kind === "client" && (
        <Field label={strings.projectsCustomer} hint={strings.projectsNewCustomerHint}>
          <CustomerPicker customers={customers} value={customerId} onChange={setCustomerId} />
        </Field>
      )}
      <div className="border-t border-subtle pt-1">
        <p className="mb-1 text-sm font-semibold text-primary">{strings.projectsDetailsTitle}</p>
        <p className="text-xs leading-5 text-secondary">{strings.projectsDetailsSubtitle}</p>
      </div>
      <Field label={strings.projectsDescription}>
        <textarea
          className="min-h-24 w-full resize-y rounded-md border border-default bg-surface px-3 py-2 text-sm leading-6 text-primary focus-visible:outline-2 focus-visible:outline-accent"
          value={description}
          maxLength={2000}
          onChange={(event) => setDescription(event.target.value)}
        />
      </Field>
      <ProjectStatusSchedule status={status} startsOn={startsOn} targetOn={targetOn} datesValid={datesValid} onStatusChange={setStatus} onStartsOnChange={setStartsOn} onTargetOnChange={setTargetOn} />
    </DialogFrame>
  );
}
