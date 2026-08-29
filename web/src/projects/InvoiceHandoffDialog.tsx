import { CalendarDays, Check, FileText, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { Button, IconButton, Input, Modal } from "../ds";
import { strings } from "../i18n";
import { amountLabel, durationLabel } from "./format";
import { projectsMessage, useProjectsApi } from "./api";
import type { Project, UnbilledTimeGroup } from "./types";

interface Props {
  project: Project;
  initialCutoff?: string;
  onClose: () => void;
  onCreated: (invoiceId: string) => void;
}

export function InvoiceHandoffDialog({
  project,
  initialCutoff,
  onClose,
  onCreated,
}: Props) {
  const api = useProjectsApi();
  const [groups, setGroups] = useState<UnbilledTimeGroup[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cutoff, setCutoff] = useState(
    () => initialCutoff ?? new Date().toISOString().slice(0, 10),
  );
  const customerId = project.client?.customerId;

  useEffect(() => {
    if (customerId === undefined) return;
    let active = true;
    setLoading(true);
    setError(null);
    void api
      .unbilledTime(customerId, cutoff)
      .then((result) => {
        if (!active) return;
        setGroups(result.groups);
        const projectEntries = new Set(
          result.groups
            .filter((group) => group.projectId === project.id)
            .flatMap((group) => group.entryIds),
        );
        setSelected(projectEntries);
      })
      .catch((cause: unknown) => {
        if (active)
          setError(projectsMessage(cause, strings.projectsInvoiceLoadFailed));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [api, cutoff, customerId, project.id]);

  const selectedGroups = useMemo(
    () =>
      groups.filter((group) => group.entryIds.some((id) => selected.has(id))),
    [groups, selected],
  );
  const selectedIds = selectedGroups.flatMap((group) =>
    group.entryIds.filter((id) => selected.has(id)),
  );

  function toggle(group: UnbilledTimeGroup) {
    setSelected((current) => {
      const next = new Set(current);
      const include = !group.entryIds.every((id) => next.has(id));
      group.entryIds.forEach((id) =>
        include ? next.add(id) : next.delete(id),
      );
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
    <Modal
      title={strings.projectsCreateInvoice}
      onClose={onClose}
      wide
      icon={<FileText size={19} />}
      actions={
        <IconButton
          label={strings.close}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          <span className="flex-1" />
          <Button variant="ghost" onClick={onClose} disabled={saving}>
            {strings.cancel}
          </Button>
          <Button
            disabled={selectedIds.length === 0 || saving}
            onClick={() => void createDraft()}
          >
            {saving
              ? strings.billingSaving
              : strings.projectsCreateDraftInvoice}
          </Button>
        </>
      }
    >
      <p className="m-0 text-sm text-secondary">
        {strings.projectsCreateInvoiceSubtitle}
      </p>
      <div>
          <div className="mb-5 flex flex-wrap items-end justify-between gap-3 rounded-xl border border-subtle bg-raised p-4">
            <div className="min-w-0">
              <p className="font-semibold text-primary">{project.name}</p>
              <p className="mt-1 text-sm text-secondary">
                {strings.projectsInvoiceCutoffHint}
              </p>
            </div>
            <label className="block shrink-0 text-sm font-medium text-primary">
              <span className="mb-1.5 block">
                {strings.projectsInvoiceThrough}
              </span>
              <span className="relative block">
                <CalendarDays
                  className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary"
                  size={17}
                />
                <Input
                  type="date"
                  value={cutoff}
                  max={new Date().toISOString().slice(0, 10)}
                  onChange={(event) => setCutoff(event.target.value)}
                  className="!pl-10"
                />
              </span>
            </label>
          </div>
          {loading ? (
            <p className="py-10 text-center text-sm text-secondary">
              {strings.chatLoading}
            </p>
          ) : groups.length === 0 ? (
            <div className="py-10 text-center">
              <p className="font-semibold text-primary">
                {strings.projectsNothingToInvoice}
              </p>
              <p className="mx-auto mt-2 max-w-md text-sm leading-6 text-secondary">
                {strings.projectsNothingToInvoiceBody}
              </p>
            </div>
          ) : (
            <div className="space-y-3">
              {groups.map((group) => {
                const checked = group.entryIds.every((id) => selected.has(id));
                return (
                  <button
                    key={`${group.projectId}-${group.rateCents ?? "unrated"}`}
                    type="button"
                    aria-pressed={checked}
                    onClick={() => toggle(group)}
                    className={`flex w-full items-center gap-4 rounded-xl border p-4 text-left !no-underline transition-colors hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${checked ? "border-accent bg-accent-soft" : "border-subtle bg-surface hover:bg-raised"}`}
                  >
                    <span
                      className={`inline-flex size-6 shrink-0 items-center justify-center rounded-md border ${checked ? "border-accent bg-accent text-on-accent" : "border-subtle bg-surface"}`}
                    >
                      {checked && <Check size={15} />}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-semibold text-primary">
                        {group.projectName}
                      </span>
                      <span className="mt-1 block text-sm text-secondary">
                        {durationLabel(group.minutes)} ·{" "}
                        {group.rateCents === null || group.currency === null
                          ? strings.projectsUnratedTime
                          : strings.projectsInvoiceRate(
                              amountLabel(group.rateCents, group.currency),
                            )}
                      </span>
                    </span>
                    <span className="shrink-0 font-semibold tabular-nums text-primary">
                      {group.netCents === null || group.currency === null
                        ? "—"
                        : amountLabel(group.netCents, group.currency)}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
          <div className="mt-5 flex items-center gap-2 rounded-xl bg-raised px-4 py-3 text-sm text-secondary">
            <span className="font-semibold text-primary">21% VAT</span>
            <span>·</span>
            <span>{strings.projectsBelgianVat}</span>
          </div>
          {error !== null && (
            <p className="mt-4 text-sm text-danger" role="alert">
              {error}
            </p>
          )}
      </div>
    </Modal>
  );
}
