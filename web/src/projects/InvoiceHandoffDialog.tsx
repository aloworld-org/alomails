import { Check, FileText, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { strings } from "../i18n";
import { amountLabel, durationLabel } from "./format";
import { projectsMessage, useProjectsApi } from "./api";
import type { Project, UnbilledTimeGroup } from "./types";

interface Props {
  project: Project;
  onClose: () => void;
  onCreated: (invoiceId: string) => void;
}

export function InvoiceHandoffDialog({ project, onClose, onCreated }: Props) {
  const api = useProjectsApi();
  const [groups, setGroups] = useState<UnbilledTimeGroup[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const customerId = project.client?.customerId;

  useEffect(() => {
    if (customerId === undefined) return;
    let active = true;
    void api.unbilledTime(customerId).then((result) => {
      if (!active) return;
      const matching = result.groups.filter((group) => group.projectId === project.id);
      setGroups(matching);
      setSelected(new Set(matching.flatMap((group) => group.entryIds)));
    }).catch((cause: unknown) => {
      if (active) setError(projectsMessage(cause, strings.projectsInvoiceLoadFailed));
    }).finally(() => {
      if (active) setLoading(false);
    });
    return () => { active = false; };
  }, [api, customerId, project.id]);

  const selectedGroups = useMemo(() => groups.filter((group) =>
    group.entryIds.some((id) => selected.has(id))), [groups, selected]);
  const selectedIds = selectedGroups.flatMap((group) => group.entryIds.filter((id) => selected.has(id)));

  function toggle(group: UnbilledTimeGroup) {
    setSelected((current) => {
      const next = new Set(current);
      const include = !group.entryIds.every((id) => next.has(id));
      group.entryIds.forEach((id) => include ? next.add(id) : next.delete(id));
      return next;
    });
  }

  async function createDraft() {
    if (customerId === undefined || selectedIds.length === 0 || saving) return;
    setSaving(true);
    setError(null);
    try {
      const draft = await api.createTimeInvoice(customerId, selectedIds);
      onCreated(draft.id);
    } catch (cause) {
      setError(projectsMessage(cause, strings.projectsInvoiceCreateFailed));
      setSaving(false);
    }
  }

  return (
    <div className="fixed inset-0 z-modal flex items-center justify-center bg-scrim p-4" role="presentation" onMouseDown={onClose}>
      <section className="flex max-h-[min(44rem,calc(100vh-2rem))] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-subtle bg-surface shadow-xl" role="dialog" aria-modal="true" aria-labelledby="invoice-handoff-title" onMouseDown={(event) => event.stopPropagation()}>
        <header className="flex items-start justify-between gap-4 border-b border-subtle px-6 py-5">
          <div className="flex min-w-0 gap-3">
            <span className="inline-flex size-10 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent"><FileText size={19} /></span>
            <div><h2 id="invoice-handoff-title" className="text-lg font-semibold text-primary">{strings.projectsCreateInvoice}</h2><p className="mt-1 text-sm text-secondary">{strings.projectsCreateInvoiceSubtitle}</p></div>
          </div>
          <button type="button" className="inline-flex size-10 shrink-0 items-center justify-center rounded-lg text-secondary !no-underline hover:bg-raised hover:text-primary hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" onClick={onClose} aria-label={strings.close}><X size={19} /></button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto p-6">
          {loading ? <p className="py-10 text-center text-sm text-secondary">{strings.chatLoading}</p> : groups.length === 0 ? (
            <div className="py-10 text-center"><p className="font-semibold text-primary">{strings.projectsNothingToInvoice}</p><p className="mx-auto mt-2 max-w-md text-sm leading-6 text-secondary">{strings.projectsNothingToInvoiceBody}</p></div>
          ) : <div className="space-y-3">{groups.map((group) => {
            const checked = group.entryIds.every((id) => selected.has(id));
            return <button key={`${group.projectId}-${group.rateCents ?? "unrated"}`} type="button" onClick={() => toggle(group)} className={`flex w-full items-center gap-4 rounded-xl border p-4 text-left !no-underline transition-colors hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${checked ? "border-accent bg-accent-soft" : "border-subtle bg-surface hover:bg-raised"}`}>
              <span className={`inline-flex size-6 shrink-0 items-center justify-center rounded-md border ${checked ? "border-accent bg-accent text-on-accent" : "border-subtle bg-surface"}`}>{checked && <Check size={15} />}</span>
              <span className="min-w-0 flex-1"><span className="block font-semibold text-primary">{durationLabel(group.minutes)}</span><span className="mt-1 block text-sm text-secondary">{group.rateCents === null || group.currency === null ? strings.projectsUnratedTime : strings.projectsInvoiceRate(amountLabel(group.rateCents, group.currency))}</span></span>
              <span className="shrink-0 font-semibold tabular-nums text-primary">{group.netCents === null || group.currency === null ? "—" : amountLabel(group.netCents, group.currency)}</span>
            </button>;
          })}</div>}
          <div className="mt-5 flex items-center gap-2 rounded-xl bg-raised px-4 py-3 text-sm text-secondary"><span className="font-semibold text-primary">21% VAT</span><span>·</span><span>{strings.projectsBelgianVat}</span></div>
          {error !== null && <p className="mt-4 text-sm text-danger" role="alert">{error}</p>}
        </div>
        <footer className="flex items-center justify-end gap-3 border-t border-subtle px-6 py-4">
          <button type="button" className="inline-flex min-h-10 items-center rounded-lg bg-raised px-4 py-2 text-sm font-medium text-primary !no-underline hover:bg-default hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" onClick={onClose}>{strings.cancel}</button>
          <button type="button" className="inline-flex min-h-10 items-center rounded-lg bg-accent px-5 py-2 text-sm font-semibold text-on-accent !no-underline hover:bg-accent-hover hover:!no-underline disabled:cursor-not-allowed disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2" disabled={selectedIds.length === 0 || saving} onClick={() => void createDraft()}>{saving ? strings.billingSaving : strings.projectsCreateDraftInvoice}</button>
        </footer>
      </section>
    </div>
  );
}
