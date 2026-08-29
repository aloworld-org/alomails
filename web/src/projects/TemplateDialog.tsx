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
import { Button, Field, IconButton, Input, Modal, Select } from "../ds";
import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import { DialogFrame } from "./parts";
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
      <Modal
        title={strings.projectsTemplateNewTitle}
        onClose={onClose}
        icon={<CopyPlus size={19} />}
        actions={
          <IconButton
            label={strings.projectsCancel}
            icon={<X size={18} />}
            onClick={onClose}
          />
        }
      >
        <p className="m-0 text-sm text-tertiary">{strings.projectsTemplateNewSubtitle}</p>
        <div className="flex flex-col items-center px-8 py-8 text-center max-sm:px-5">
          <span className="flex size-14 items-center justify-center rounded-2xl bg-accent-soft text-accent">
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
      </Modal>
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
            {(control) => (
              <Select
                id={control.id}
                aria-describedby={control["aria-describedby"]}
                fullWidth
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
              </Select>
            )}
          </Field>

          <Field label={strings.projectsTemplateName} hint={strings.projectsTemplateNameHint}>
            {(control) => (
              <Input
                id={control.id}
                aria-describedby={control["aria-describedby"]}
                value={name}
                maxLength={120}
                onChange={(e) => setName(e.target.value)}
              />
            )}
          </Field>

          <div className="grid grid-cols-2 gap-4 max-sm:grid-cols-1">
            <Field label={strings.projectsTemplateStarts} hint={strings.projectsTemplateStartsHint}>
              {(control) => (
                <Input
                  id={control.id}
                  aria-describedby={control["aria-describedby"]}
                  type="date"
                  value={startsOn}
                  onChange={(e) => setStartsOn(e.target.value)}
                />
              )}
            </Field>
            <Field
              label={strings.projectsCustomer}
              hint={strings.projectsTemplateCustomerHint}
            >
              {/* An empty customer is a real answer — the copy is internal
                  work — so the empty option stays choosable. */}
              {(control) => (
                <Select
                  id={control.id}
                  aria-describedby={control["aria-describedby"]}
                  fullWidth
                  placeholder={strings.projectsTemplateNoCustomer}
                  value={customerId}
                  onChange={(e) => setCustomerId(e.target.value)}
                >
                  {customers.map((customer) => (
                    <option key={customer.id} value={customer.id}>
                      {customer.name}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
          </div>

          {chosen !== null && chosen.milestoneCount === 0 && (
            <p className="m-0 text-sm leading-5 text-tertiary">{strings.projectsTemplateNoPlan}</p>
          )}
      </>
    </DialogFrame>
  );
}
