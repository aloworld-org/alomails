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
import { CopyPlus } from "lucide-react";

import { useCustomers } from "../billing";
import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import { DialogFrame, EmptyState, Field } from "./parts";
import type { ProjectTemplate, TemplateCopy } from "./types";
import styles from "./ProjectsModule.module.css";

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
      {templates.length === 0 ? (
        <EmptyState
          Icon={CopyPlus}
          title={strings.projectsTemplateEmptyTitle}
          body={strings.projectsTemplateEmptyBody}
        />
      ) : (
        <>
          <Field label={strings.projectsTemplateWhich} hint={strings.projectsTemplateWhichHint}>
            <select
              className={styles.select}
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
              className={styles.input}
              value={name}
              maxLength={120}
              onChange={(e) => setName(e.target.value)}
            />
          </Field>

          <div className={styles.row}>
            <Field label={strings.projectsTemplateStarts} hint={strings.projectsTemplateStartsHint}>
              <input
                className={styles.input}
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
                className={styles.select}
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
            <p className={styles.hint}>{strings.projectsTemplateNoPlan}</p>
          )}
        </>
      )}
    </DialogFrame>
  );
}
