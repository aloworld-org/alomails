// The engagement form: who a project is worked for, in what currency, at what
// rate, against what budget — and the way back to internal work.
//
// This is the form the whole wave was missing. Nothing before it could say who
// a project is worked for, so an hour logged against it had no rate to
// snapshot and no client to be billable to.
//
// Three rules the screen keeps, all of them the server's:
//
// - **Whole record.** The server's write is one idempotent replacement, so a
//   field left blank is *cleared*, not kept. The form is loaded with what is
//   stored and saved as it stands, which is what "save" has to mean for a form
//   that shows every field.
// - **An unpriced engagement is legal.** A blank rate is `null`, never `0` —
//   the difference between "nobody has priced this" and "this is free", and the
//   one the handoff to an invoice refuses to guess at.
// - **Only a team board can be client work.** A personal board is refused by
//   the server naming the rule; the form says so up front rather than offering
//   a save that always fails.
//
// No arithmetic: the rate and both budgets are typed as decimals and sent as
// the integers the API takes, and every figure shown back comes from the
// server's answer.
import { useState } from "react";
import { Briefcase } from "lucide-react";

import { hundredthsToInput, parseHundredths, useCustomers } from "../billing";
import { useDialogs } from "../ds";
import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import { DialogFrame, Field } from "./parts";
import type { Project } from "./types";

/** Minutes typed as whole hours, and back. A budget is stated in hours by
 *  everybody who has ever agreed one; the API holds minutes so it can be
 *  compared with an entry without a second scale. */
function hoursToInput(minutes: number | null): string {
  return minutes === null ? "" : String(Math.round(minutes / 60));
}

export function ClientDialog({
  project,
  onClose,
  onSaved,
}: {
  project: Project;
  onClose: () => void;
  onSaved: () => void;
}) {
  const api = useProjectsApi();
  const dialogs = useDialogs();
  const { customers, error: customersError } = useCustomers();
  const client = project.client;

  const [customerId, setCustomerId] = useState(client?.customerId ?? "");
  const [rate, setRate] = useState(
    client?.rateCents === undefined || client.rateCents === null
      ? ""
      : hundredthsToInput(client.rateCents),
  );
  const [budgetHours, setBudgetHours] = useState(hoursToInput(client?.budgetMinutes ?? null));
  const [budgetAmount, setBudgetAmount] = useState(
    client?.budgetCents === undefined || client.budgetCents === null
      ? ""
      : hundredthsToInput(client.budgetCents),
  );
  const [startsOn, setStartsOn] = useState(client?.startsOn ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const teamProject = project.kind === "team";
  // What the edge could not turn into what the API takes. Each is checked as
  // typed so the refusal arrives beside the field, not after a round trip.
  const rateError =
    rate.trim() !== "" && parseHundredths(rate) === null ? strings.projectsRateInvalid : undefined;
  const budgetHoursError =
    budgetHours.trim() !== "" && !/^[0-9]{1,8}$/.test(budgetHours.trim())
      ? strings.projectsBudgetHoursInvalid
      : undefined;
  const budgetAmountError =
    budgetAmount.trim() !== "" && parseHundredths(budgetAmount) === null
      ? strings.projectsBudgetAmountInvalid
      : undefined;
  const canSubmit =
    teamProject &&
    customerId !== "" &&
    rateError === undefined &&
    budgetHoursError === undefined &&
    budgetAmountError === undefined;

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await api.setClient(project.id, {
        customerId,
        // Blank means absent, and absent is cleared — the whole-record rule.
        rateCents: rate.trim() === "" ? null : parseHundredths(rate),
        budgetMinutes: budgetHours.trim() === "" ? null : Number(budgetHours.trim()) * 60,
        budgetCents: budgetAmount.trim() === "" ? null : parseHundredths(budgetAmount),
        startsOn: startsOn === "" ? null : startsOn,
      });
      onSaved();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
      setBusy(false);
    }
  }

  async function detach() {
    // Undo over confirm is the house rule, but this one is asked: the hours
    // stay and only the claim that they are billable goes, and a person who
    // has not read that sentence would read a silent change as data loss.
    const sure = await dialogs.confirm({
      title: strings.projectsDetachTitle,
      message: strings.projectsDetachBody,
      confirmLabel: strings.projectsDetach,
      danger: true,
    });
    if (!sure) return;
    setBusy(true);
    setError(null);
    try {
      await api.clearClient(project.id);
      onSaved();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Briefcase}
      title={project.name}
      subtitle={strings.projectsClientSubtitle}
      error={error ?? customersError}
      busy={busy}
      canSubmit={canSubmit}
      submitLabel={strings.projectsSave}
      extraAction={
        client === null ? undefined : { label: strings.projectsDetach, onClick: () => void detach() }
      }
      onClose={onClose}
      onSubmit={() => void save()}
    >
      {!teamProject && <p className="text-xs text-tertiary">{strings.projectsPersonalBoard}</p>}

      <Field label={strings.projectsCustomer} hint={strings.projectsCustomerHint}>
        <select
          className="w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent"
          value={customerId}
          disabled={!teamProject}
          onChange={(e) => setCustomerId(e.target.value)}
        >
          <option value="">{strings.projectsCustomerPick}</option>
          {customers.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
      </Field>

      <div className="flex flex-wrap gap-3 [&>*]:min-w-40 [&>*]:flex-1">
        <Field label={strings.projectsRate} hint={strings.projectsRateHint} error={rateError}>
          <input
            className="w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent"
            inputMode="decimal"
            value={rate}
            disabled={!teamProject}
            onChange={(e) => setRate(e.target.value)}
          />
        </Field>
        <Field label={strings.projectsStartsOn}>
          <input
            className="w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent"
            type="date"
            value={startsOn}
            disabled={!teamProject}
            onChange={(e) => setStartsOn(e.target.value)}
          />
        </Field>
      </div>

      <div className="flex flex-wrap gap-3 [&>*]:min-w-40 [&>*]:flex-1">
        <Field
          label={strings.projectsBudgetHours}
          hint={strings.projectsBudgetHint}
          error={budgetHoursError}
        >
          <input
            className="w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent"
            inputMode="numeric"
            value={budgetHours}
            disabled={!teamProject}
            onChange={(e) => setBudgetHours(e.target.value)}
          />
        </Field>
        <Field label={strings.projectsBudgetAmount} error={budgetAmountError}>
          <input
            className="w-full rounded-md border border-default bg-surface px-3 py-2 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent"
            inputMode="decimal"
            value={budgetAmount}
            disabled={!teamProject}
            onChange={(e) => setBudgetAmount(e.target.value)}
          />
        </Field>
      </div>
    </DialogFrame>
  );
}
