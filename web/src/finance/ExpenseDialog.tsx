// The claim form: what somebody spent, on what day, out of whose pocket.
//
// It is the same form for recording a claim and for correcting one, because the
// server takes the same shape for both — a `POST` that states everything and a
// `PATCH` that states everything it shows. Two forms would be two places for the
// rules to drift.
//
// Three things this form deliberately does NOT do:
//
// - **It computes no money.** The net of a claim is gross less VAT, and it is
//   the server's subtraction: the form shows what came back, never its own sum.
//   What it does own is the edge where a person types "11,90" and the API wants
//   `1190` — Billing's parser, shared, so two modules cannot read a comma two
//   ways.
// - **It invents no currency.** An empty currency box means "the workspace's
//   own", which is the server's default; filling one in for the user would be a
//   browser deciding what money a receipt is in.
// - **It re-states no server rule.** The bounds on an amount, the rule that VAT
//   may not exceed the gross, the length of a merchant name: all are the
//   store's, and a refusal is shown in the server's own sentence. The only
//   checks here are the ones that stop a person losing what they typed — a day
//   that is not a day, an amount that is not a number.
import { useState } from "react";
import { ReceiptText } from "lucide-react";

import { RecordAgentPanel } from "../agents";
import { hundredthsToInput, parseHundredths } from "../billing";
import { DatePicker, Field, Input, Select } from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { today } from "./format";
import { DialogFrame } from "./parts";
import type { Expense, ExpenseDraft, ExpenseMethod } from "./types";
import styles from "./FinanceModule.module.css";

/** One project a claim can be attached to. Just the two facts a picker needs,
 *  so this dialog does not depend on the whole Projects record. */
export interface ProjectChoice {
  id: string;
  name: string;
}

/** The three words the API takes for "whose money paid", in the order a form
 *  offers them: out of pocket first, because it is the only one that ends in
 *  somebody being owed money. */
const METHODS: ExpenseMethod[] = ["personal", "card", "cash"];

/** What the boxes hold: text, exactly as typed, until it is sent. */
interface Form {
  spentOn: string;
  merchant: string;
  description: string;
  gross: string;
  vat: string;
  vatRate: string;
  currency: string;
  method: ExpenseMethod;
  projectId: string;
}

/** The stored claim as text in the boxes, or an empty claim dated today. */
function formOf(claim: Expense | null): Form {
  if (claim === null) {
    return {
      spentOn: today(),
      merchant: "",
      description: "",
      gross: "",
      vat: "",
      vatRate: "",
      currency: "",
      method: "personal",
      projectId: "",
    };
  }
  return {
    spentOn: claim.spentOn,
    merchant: claim.merchant,
    description: claim.description,
    gross: hundredthsToInput(claim.grossCents),
    vat: claim.vatCents === 0 ? "" : hundredthsToInput(claim.vatCents),
    vatRate: claim.vatRateBp === null ? "" : hundredthsToInput(claim.vatRateBp),
    currency: claim.currency,
    method: claim.method,
    projectId: claim.projectId ?? "",
  };
}

export function ExpenseDialog({
  claim,
  projects,
  onClose,
  onSaved,
  onDelete,
}: {
  /** The claim being corrected, or `null` to record a new one. */
  claim: Expense | null;
  projects: ProjectChoice[];
  onClose: () => void;
  onSaved: () => void;
  /** Offered only for a claim that is still the claimant's own; absent for a
   *  new one, which cannot be deleted before it exists. */
  onDelete?: (() => void) | undefined;
}) {
  const api = useFinanceApi();
  const [form, setForm] = useState<Form>(() => formOf(claim));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const grossCents = parseHundredths(form.gross);
  const vatCents = form.vat.trim() === "" ? 0 : parseHundredths(form.vat);
  const vatRateBp =
    form.vatRate.trim() === "" ? null : parseHundredths(form.vatRate);
  const grossError =
    form.gross.trim() !== "" && grossCents === null
      ? strings.financeAmountInvalid
      : undefined;
  const vatError =
    form.vat.trim() !== "" && vatCents === null
      ? strings.financeAmountInvalid
      : undefined;
  const rateError =
    form.vatRate.trim() !== "" && vatRateBp === null
      ? strings.financeRateInvalid
      : undefined;
  const canSubmit =
    form.spentOn !== "" &&
    grossCents !== null &&
    vatCents !== null &&
    grossError === undefined &&
    vatError === undefined &&
    rateError === undefined;

  function set<K extends keyof Form>(key: K, value: Form[K]) {
    setForm((current) => ({ ...current, [key]: value }));
  }

  async function save() {
    if (grossCents === null || vatCents === null) return;
    const draft: ExpenseDraft = {
      spentOn: form.spentOn,
      merchant: form.merchant.trim(),
      description: form.description.trim(),
      grossCents,
      vatCents,
      vatRateBp,
      method: form.method,
      projectId: form.projectId === "" ? null : form.projectId,
      ...(form.currency.trim() === ""
        ? {}
        : { currency: form.currency.trim().toUpperCase() }),
    };
    setBusy(true);
    setError(null);
    try {
      if (claim === null) await api.createExpense(draft);
      else await api.updateExpense(claim.id, draft);
      onSaved();
    } catch (err) {
      setError(financeMessage(err, strings.financeSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={ReceiptText}
      title={
        claim === null ? strings.financeNewClaim : strings.financeEditClaim
      }
      subtitle={strings.financeClaimSubtitle}
      error={error}
      busy={busy}
      canSubmit={canSubmit}
      submitLabel={strings.financeSave}
      extraAction={
        onDelete === undefined
          ? undefined
          : { label: strings.financeDelete, onClick: onDelete }
      }
      onClose={onClose}
      onSubmit={() => void save()}
      aside={
        claim !== null && (
          <RecordAgentPanel
            product="finance"
            recordKind="expense"
            recordId={claim.id}
            recordLabel={
              claim.merchant === "" ? claim.description : claim.merchant
            }
            origin={null}
            onBeforeNavigate={onClose}
          />
        )
      }
    >
      <div className={styles.row}>
        <Field label={strings.financeSpentOn} hint={strings.financeSpentOnHint}>
          {(control) => (
            <DatePicker
              {...control}
              value={form.spentOn}
              onChange={(spentOn) => set("spentOn", spentOn)}
            />
          )}
        </Field>
        <Field label={strings.financeMethod} hint={strings.financeMethodHint}>
          {(control) => (
            <Select
              {...control}
              fullWidth
              value={form.method}
              onChange={(e) => set("method", e.target.value as ExpenseMethod)}
            >
              {METHODS.map((method) => (
                <option key={method} value={method}>
                  {methodOption(method)}
                </option>
              ))}
            </Select>
          )}
        </Field>
      </div>

      <Field label={strings.financeMerchant} hint={strings.financeMerchantHint}>
        {(control) => (
          <Input
            {...control}
            value={form.merchant}
            onChange={(e) => set("merchant", e.target.value)}
          />
        )}
      </Field>

      {/* The one control here that is not the design system's. `ds/` has no
          multi-line text control yet, so the box is still drawn locally rather
          than approximated with a taller `Input` — flagged for D3.01, where the
          four modules waiting on one are counted. */}
      <Field label={strings.financeDescription}>
        {(control) => (
          <textarea
            id={control.id}
            aria-describedby={control["aria-describedby"]}
            className={styles.textarea}
            value={form.description}
            onChange={(e) => set("description", e.target.value)}
          />
        )}
      </Field>

      <div className={styles.row}>
        <Field label={strings.financeGross} error={grossError}>
          {(control) => (
            <Input
              {...control}
              inputMode="decimal"
              value={form.gross}
              onChange={(e) => set("gross", e.target.value)}
              required
            />
          )}
        </Field>
        <Field
          label={strings.financeVat}
          hint={strings.financeVatHint}
          error={vatError}
        >
          {(control) => (
            <Input
              {...control}
              inputMode="decimal"
              value={form.vat}
              onChange={(e) => set("vat", e.target.value)}
            />
          )}
        </Field>
        <Field
          label={strings.financeVatRate}
          hint={strings.financeVatRateHint}
          error={rateError}
        >
          {(control) => (
            <Input
              {...control}
              inputMode="decimal"
              value={form.vatRate}
              onChange={(e) => set("vatRate", e.target.value)}
            />
          )}
        </Field>
      </div>

      <div className={styles.row}>
        <Field
          label={strings.financeCurrency}
          hint={strings.financeCurrencyHint}
        >
          {(control) => (
            <Input
              {...control}
              value={form.currency}
              maxLength={3}
              onChange={(e) => set("currency", e.target.value)}
            />
          )}
        </Field>
        <Field label={strings.financeProject} hint={strings.financeProjectHint}>
          {(control) => (
            // "No engagement" is an answer, not a prompt: most claims have none.
            <Select
              {...control}
              fullWidth
              placeholder={strings.financeNoProject}
              value={form.projectId}
              onChange={(e) => set("projectId", e.target.value)}
            >
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </Select>
          )}
        </Field>
      </div>
    </DialogFrame>
  );
}

/** The word for a payment method on the picker. Kept beside the picker rather
 *  than in `format.ts`, because these read as choices ("I paid") while the list
 *  reads as a fact about a claim. */
function methodOption(method: ExpenseMethod): string {
  switch (method) {
    case "personal":
      return strings.financeMethodPersonalOption;
    case "card":
      return strings.financeMethodCardOption;
    default:
      return strings.financeMethodCashOption;
  }
}
