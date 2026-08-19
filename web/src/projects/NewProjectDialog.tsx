import { useEffect, useRef, useState } from "react";
import { BriefcaseBusiness, Building2, Check, ChevronDown, UserRound } from "lucide-react";

import { strings } from "../i18n";
import { DialogFrame, Field } from "./parts";

export interface NewProjectDraft {
  name: string;
  customerId: string | null;
}

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
        className={`flex min-h-11 w-full items-center justify-between gap-3 rounded-md border bg-surface px-4 py-2 text-left text-sm transition-colors focus-visible:outline-2 focus-visible:outline-accent ${open ? "border-accent ring-1 ring-accent" : "border-default hover:border-strong"}`}
        onClick={() => setOpen((current) => !current)}
      >
        <span className={selected === null ? "text-tertiary" : "font-medium text-primary"}>
          {selected?.name ?? strings.projectsCustomerPick}
        </span>
        <ChevronDown className={`size-4 shrink-0 text-secondary transition-transform ${open ? "rotate-180" : ""}`} aria-hidden="true" />
      </button>
      {open && (
        <div
          id="new-project-customer-list"
          role="listbox"
          aria-label={strings.projectsCustomer}
          className="absolute inset-x-0 top-full z-[var(--z-overlay)] mt-2 max-h-56 overflow-y-auto rounded-lg border border-subtle bg-surface p-1.5 shadow-lg"
        >
          {customers.map((customer) => {
            const active = customer.id === value;
            return (
              <button
                key={customer.id}
                type="button"
                role="option"
                aria-selected={active}
                className={`flex min-h-10 w-full items-center justify-between gap-3 rounded-md px-3 py-2 text-left text-sm font-medium transition-colors ${active ? "bg-accent-soft text-accent" : "text-primary hover:bg-raised"}`}
                onClick={() => {
                  onChange(customer.id);
                  setOpen(false);
                }}
              >
                <span className="truncate">{customer.name}</span>
                {active && <Check className="size-4 shrink-0" aria-hidden="true" />}
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
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const canSubmit = name.trim() !== "" && (kind === "internal" || customerId !== "");

  async function create() {
    setBusy(true);
    setError(null);
    try {
      await onCreate({ name: name.trim(), customerId: kind === "client" ? customerId : null });
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
        <div className="grid grid-cols-2 gap-3">
          {(["client", "internal"] as const).map((option) => {
            const selected = kind === option;
            const Icon = option === "client" ? UserRound : Building2;
            return (
              <button
                key={option}
                type="button"
                className={`relative flex min-h-16 items-center gap-3 rounded-lg border px-4 py-3 text-left text-sm font-semibold transition-colors focus-visible:outline-2 focus-visible:outline-accent ${selected ? "border-accent bg-accent-soft text-primary" : "border-default bg-surface text-secondary hover:border-strong hover:bg-raised hover:text-primary"}`}
                aria-pressed={selected}
                onClick={() => setKind(option)}
              >
                <span className={`flex size-8 shrink-0 items-center justify-center rounded-md ${selected ? "bg-accent text-on-accent" : "bg-raised text-secondary"}`}>
                  <Icon className="size-4" aria-hidden="true" />
                </span>
                <span className="flex-1">{option === "client" ? strings.projectsClientWork : strings.projectsInternalWork}</span>
                {selected && <Check className="size-4 shrink-0 text-accent" aria-hidden="true" />}
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
    </DialogFrame>
  );
}
