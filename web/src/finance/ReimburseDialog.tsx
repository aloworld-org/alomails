// Marking a claim paid back: the one fact the server needs is the day the money
// actually moved.
//
// It is a form rather than a confirmation because that day is a real input and
// not a formality — it is the date the reimbursement books on, and a payment
// entered in the wrong period is a correction somebody has to file. The box
// opens on today in the reader's own zone (the common case: the transfer just
// went out), and a payer who is catching up on last week's payments changes it.
//
// The server refuses a claim that is not approved, and one nobody is owed
// anything on, with its own sentence — repeated by nothing here.
import { useState } from "react";
import { Banknote } from "lucide-react";

import { Field, Input } from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { amountLabel, today } from "./format";
import { DialogFrame } from "./parts";
import type { PendingExpense } from "./types";

export function ReimburseDialog({
  claim,
  onClose,
  onDone,
}: {
  claim: PendingExpense;
  onClose: () => void;
  onDone: () => void;
}) {
  const api = useFinanceApi();
  const [day, setDay] = useState(() => today());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function pay() {
    setBusy(true);
    setError(null);
    try {
      await api.reimburseExpense(claim.id, day);
      onDone();
    } catch (err) {
      setError(financeMessage(err, strings.financeSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Banknote}
      title={strings.financeMarkPaidBack}
      subtitle={strings.financeMarkPaidBackSubtitle(
        claim.userEmail,
        amountLabel(claim.grossCents, claim.currency),
      )}
      error={error}
      busy={busy}
      canSubmit={day !== ""}
      submitLabel={strings.financeMarkPaidBack}
      onClose={onClose}
      onSubmit={() => void pay()}
    >
      <Field
        label={strings.financeReimbursedOn}
        hint={strings.financeReimbursedOnHint}
      >
        {(control) => (
          <Input
            {...control}
            type="date"
            value={day}
            onChange={(e) => setDay(e.target.value)}
            required
          />
        )}
      </Field>
    </DialogFrame>
  );
}
