import { useState } from "react";
import { BriefcaseBusiness } from "lucide-react";

import { strings } from "../i18n";
import { DialogFrame, Field } from "./parts";

export interface NewProjectDraft {
  name: string;
  customerId: string | null;
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
        <div className="grid grid-cols-2 gap-2 rounded-lg bg-raised p-1">
          {(["client", "internal"] as const).map((option) => {
            const selected = kind === option;
            return <button key={option} type="button" className={`min-h-11 rounded-md border px-4 py-2 text-sm font-medium transition-colors ${selected ? "border-accent bg-surface text-accent shadow-sm" : "border-transparent bg-transparent text-secondary hover:bg-surface hover:text-primary"}`} aria-pressed={selected} onClick={() => setKind(option)}>{option === "client" ? strings.projectsClientWork : strings.projectsInternalWork}</button>;
          })}
        </div>
      </fieldset>
      {kind === "client" && (
        <Field label={strings.projectsCustomer} hint={strings.projectsNewCustomerHint}>
          <select className="min-h-11 w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent" value={customerId} onChange={(event) => setCustomerId(event.target.value)}>
            <option value="">{strings.projectsCustomerPick}</option>
            {customers.map((customer) => <option key={customer.id} value={customer.id}>{customer.name}</option>)}
          </select>
        </Field>
      )}
    </DialogFrame>
  );
}
