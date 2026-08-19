// Start a project from a template: which shape, what it is called, when it
// starts, and who it is for.
//
// The template list is the choice, shown with what each one would bring — "8
// cards · 3 milestones" — because the promise a copy makes should be readable
// before it is made, not discovered on the board afterwards. Those counts are
// the server's, and they count exactly what the copy carries: finished cards
// are not part of a shape and are neither counted here nor copied there.
//
// **The customer is asked for, never inherited.** A template is the shape of an
// engagement and not its client (`docs/design/projects.md`, "Milestones and
// templates"), so the field starts empty even when the template itself is
// client work — its currency, rate and budgets come along, and the name on the
// invoice is this form's to state. Leaving it blank is a real answer: the copy
// is internal work.
//
// No dates are computed here. The form sends the day the user picked and the
// server shifts the whole plan onto it; what comes back says how much landed.
import { useState } from "react";
import { CopyPlus, Star, X } from "lucide-react";

import { useCustomers } from "../billing";
import { Button } from "../ds";
import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import { DialogFrame, Field } from "./parts";
import type { ProjectTemplate, TemplateCopy } from "./types";

export function TemplateDialog({
  templates,
  defaultDay,
  onClose,
  onCreated,
}: {
  /** The boards marked reusable. Empty is a real state, and the dialog says how
   *  to end it rather than showing an inert form. */
  templates: ProjectTemplate[];
  /** `YYYY-MM-DD` the start field opens on — the caller's own today, so a
   *  copy made with one click still lands on a day somebody chose. */
  defaultDay: string;
  onClose: () => void;
  onCreated: (copy: TemplateCopy) => void;
}) {
  const api = useProjectsApi();
  // Archived customers excluded: this picker is choosing who the *new* work is
  // for, and the server refuses an archived one by name.
  const { customers, error: customersError } = useCustomers();

  const [templateId, setTemplateId] = useState(templates[0]?.projectId ?? "");
  const [name, setName] = useState("");
  const [startsOn, setStartsOn] = useState(defaultDay);
  const [customerId, setCustomerId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const chosen = templates.find((t) => t.projectId === templateId) ?? null;
  // The name's own rules — blank, too long — are the server's. What is checked
  // here is only that there is something to send, so the button is not offered
  // for an empty form.
  const canSubmit = chosen !== null && name.trim() !== "";

  async function create() {
    if (chosen === null) return;
    setBusy(true);
    setError(null);
    try {
      const copy = await api.instantiateTemplate(chosen.projectId, {
        name: name.trim(),
        startsOn: startsOn === "" ? null : startsOn,
        customerId: customerId === "" ? null : customerId,
      });
      onCreated(copy);
    } catch (err) {
      setError(projectsMessage(err, strings.projectsTemplateFailed));
      setBusy(false);
    }
  }

  // An empty catalogue is not a form: there is nothing a person can submit.
  // Keep that first-run state compact and give it one honest way forward
  // instead of leaving a disabled "Create project" button in the footer.
  if (templates.length === 0) {
    return (
      <div
        className="fixed inset-0 z-modal flex items-center justify-center bg-overlay p-4"
        role="presentation"
        onMouseDown={onClose}
      >
        <section
          className="w-full max-w-[39rem] overflow-hidden rounded-2xl border border-subtle bg-surface shadow-xl"
          role="dialog"
          aria-modal="true"
          aria-labelledby="empty-template-title"
          onMouseDown={(event) => event.stopPropagation()}
          onKeyDown={(event) => {
            if (event.key === "Escape") onClose();
          }}
        >
          <header className="flex items-start gap-3 border-b border-subtle px-5 py-4">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-xl bg--soft text-accent">
              <CopyPlus className="size-5" aria-hidden="true" />
            </span>
            <div className="min-w-0 flex-1">
              <h2 id="empty-template-title" className="m-0 text-lg font-semibold text-primary">
                {strings.projectsTemplateNewTitle}
              </h2>
              <p className="m-0 mt-0.5 text-sm text-tertiary">
                {strings.projectsTemplateNewSubtitle}
              </p>
            </div>
            <button
              type="button"
              className="flex size-9 shrink-0 items-center justify-center rounded-lg text-tertiary transition-colors hover:bg-raised hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
              onClick={onClose}
              aria-label={strings.projectsCancel}
            >
              <X className="size-[18px]" aria-hidden="true" />
            </button>
          </header>

          <div className="flex flex-col items-center px-8 py-8 text-center max-sm:px-5">
            <span className="flex size-14 items-center justify-center rounded-2xl bg--soft text-accent">
              <Star className="size-7" aria-hidden="true" />
            </span>
            <h3 className="m-0 mt-4 text-lg font-semibold text-primary">
              {strings.projectsTemplateEmptyTitle}
            </h3>
            <p className="m-0 mt-2 max-w-[44ch] text-sm leading-6 text-secondary">
              {strings.projectsTemplateEmptyBody}
            </p>
            <Button
              variant="primary"
              autoFocus
              icon={<Star className="size-4" aria-hidden="true" />}
              className="mt-6 h-10 rounded-xl px-4 text-sm font-semibold shadow-sm"
              onClick={onClose}
            >
              {strings.projectsTemplateChooseProject}
            </Button>
          </div>
        </section>
      </div>
    );
  }

  return (
    <DialogFrame
      Icon={CopyPlus}
      title={strings.projectsTemplateNewTitle}
      subtitle={strings.projectsTemplateNewSubtitle}
      error={error ?? customersError}
      busy={busy}
      canSubmit={canSubmit}
      submitLabel={strings.projectsTemplateCreate}
      onClose={onClose}
      onSubmit={() => void create()}
    >
      <>
          <Field label={strings.projectsTemplateWhich} hint={strings.projectsTemplateWhichHint}>
            <select
              className="h-10 w-full rounded-md border border-default bg-surface px-3 text-sm text-primary outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-accent-tint"
              autoFocus
              value={templateId}
              onChange={(e) => setTemplateId(e.target.value)}
            >
              {templates.map((template) => (
                <option key={template.projectId} value={template.projectId}>
                  {strings.projectsTemplateOption(
                    template.name,
                    template.taskCount,
                    template.milestoneCount,
                  )}
                </option>
              ))}
            </select>
          </Field>

          <Field label={strings.projectsTemplateName} hint={strings.projectsTemplateNameHint}>
            <input
              className="h-10 w-full rounded-md border border-default bg-surface px-3 text-sm text-primary outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-accent-tint"
              value={name}
              maxLength={120}
              onChange={(e) => setName(e.target.value)}
            />
          </Field>

          <div className="grid grid-cols-2 gap-4 max-sm:grid-cols-1">
            <Field label={strings.projectsTemplateStarts} hint={strings.projectsTemplateStartsHint}>
              <input
                className="h-10 w-full rounded-md border border-default bg-surface px-3 text-sm text-primary outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-accent-tint"
                type="date"
                value={startsOn}
                onChange={(e) => setStartsOn(e.target.value)}
              />
            </Field>
            <Field
              label={strings.projectsCustomer}
              hint={strings.projectsTemplateCustomerHint}
            >
              <select
                className="h-10 w-full rounded-md border border-default bg-surface px-3 text-sm text-primary outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-accent-tint"
                value={customerId}
                onChange={(e) => setCustomerId(e.target.value)}
              >
                <option value="">{strings.projectsTemplateNoCustomer}</option>
                {customers.map((customer) => (
                  <option key={customer.id} value={customer.id}>
                    {customer.name}
                  </option>
                ))}
              </select>
            </Field>
          </div>

          {chosen !== null && chosen.milestoneCount === 0 && (
            <p className="m-0 text-sm leading-5 text-tertiary">{strings.projectsTemplateNoPlan}</p>
          )}
      </>
    </DialogFrame>
  );
}
