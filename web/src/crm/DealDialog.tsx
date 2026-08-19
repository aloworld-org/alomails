// Raise a deal, or edit the one that is open.
//
// The money edge lives here, exactly as it does in the billing dialogs: a user
// types a value and the API takes integer cents, so `parseHundredths` does that
// one conversion and refuses text it cannot turn into a number. Nothing else is
// judged in the browser — a blank title, a currency that is not ISO 4217, a
// value above the ceiling and a malformed day are all the server's refusals,
// shown as the sentences it wrote.
//
// The form cannot move, reposition or close a deal: those are the move route,
// which is why a stale form left open on another screen can never win a deal.
import { useState } from "react";
import { Handshake } from "lucide-react";

import { hundredthsToInput, parseHundredths } from "../billing";
import { Field, Input } from "../ds";
import { strings } from "../i18n";
import { crmMessage, useCrmApi } from "./api";
import { DialogFrame } from "./parts";
import type { CrmDeal, DealDraft } from "./types";
import styles from "./CrmModule.module.css";

interface Props {
  /** The deal being edited, or `null` to raise one. */
  deal: CrmDeal | null;
  /** Where a new deal is raised. Ignored when editing. */
  pipelineId: string;
  stageId: string;
  onClose: () => void;
  /** The caller re-reads from the server, so the saved record is not passed
   *  on — one source of truth for what a deal now says. */
  onSaved: (deal: CrmDeal) => void;
}

export function DealDialog({
  deal,
  pipelineId,
  stageId,
  onClose,
  onSaved,
}: Props) {
  const api = useCrmApi();
  const [title, setTitle] = useState(deal?.title ?? "");
  const [company, setCompany] = useState(deal?.companyName ?? "");
  const [contactName, setContactName] = useState(deal?.contactName ?? "");
  const [contactEmail, setContactEmail] = useState(deal?.contactEmail ?? "");
  const [value, setValue] = useState(
    deal === null ? "" : hundredthsToInput(deal.valueCents),
  );
  const [currency, setCurrency] = useState(deal?.currency ?? "");
  const [expectedClose, setExpectedClose] = useState(deal?.expectedClose ?? "");
  const [source, setSource] = useState(deal?.source ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Blank means "not stated": on a create the server's default (an unpriced EUR
  // opportunity) applies, on an edit the stored value stays.
  const valueCents = value.trim() === "" ? null : parseHundredths(value);
  const valueError = value.trim() !== "" && valueCents === null;

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const draft: DealDraft = {};
      // Only what changed is sent (the module rule billing set): a PATCH that
      // replays every field would overwrite a colleague's edit with a value
      // this form read minutes ago.
      const put = <K extends keyof DealDraft>(
        key: K,
        next: DealDraft[K],
        stored: DealDraft[K],
      ) => {
        if (deal === null ? next !== "" && next !== null : next !== stored)
          draft[key] = next;
      };
      put("title", title.trim(), deal?.title);
      put("companyName", company.trim(), deal?.companyName);
      put("contactName", contactName.trim(), deal?.contactName);
      put("contactEmail", contactEmail.trim(), deal?.contactEmail);
      put("currency", currency.trim().toUpperCase(), deal?.currency);
      put("source", source.trim(), deal?.source);
      // A cleared day is an explicit `null` — "no expected close" is a decision,
      // and absent would mean "leave the old one".
      const day = expectedClose.trim() === "" ? null : expectedClose.trim();
      if (deal === null ? day !== null : day !== deal.expectedClose)
        draft.expectedClose = day;
      if (
        valueCents !== null &&
        (deal === null || valueCents !== deal.valueCents)
      ) {
        draft.valueCents = valueCents;
      }
      if (deal === null) {
        onSaved(await api.createDeal({ ...draft, pipelineId, stageId }));
      } else {
        onSaved(await api.updateDeal(deal.id, draft));
      }
    } catch (err) {
      setError(crmMessage(err, strings.crmSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Handshake}
      title={deal === null ? strings.crmNewDeal : strings.crmEditDeal}
      subtitle={strings.crmDealSubtitle}
      error={error}
      busy={busy}
      canSubmit={title.trim() !== "" && !valueError}
      submitLabel={deal === null ? strings.crmCreate : strings.crmSave}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={strings.crmFieldTitle}>
        {(control) => (
          <Input
            {...control}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            autoFocus
            required
          />
        )}
      </Field>

      <Field label={strings.crmFieldCompany} hint={strings.crmCompanyHint}>
        {(control) => (
          <Input
            {...control}
            value={company}
            onChange={(e) => setCompany(e.target.value)}
          />
        )}
      </Field>

      <div className={styles.row}>
        <Field label={strings.crmFieldContactName}>
          {(control) => (
            <Input
              {...control}
              value={contactName}
              onChange={(e) => setContactName(e.target.value)}
            />
          )}
        </Field>
        <Field
          label={strings.crmFieldContactEmail}
          hint={strings.crmContactEmailHint}
        >
          {(control) => (
            <Input
              {...control}
              type="email"
              value={contactEmail}
              onChange={(e) => setContactEmail(e.target.value)}
            />
          )}
        </Field>
      </div>

      <div className={styles.row}>
        <Field
          label={strings.crmFieldValue}
          hint={strings.crmValueHint}
          error={valueError ? strings.crmNotAnAmount : undefined}
        >
          {(control) => (
            <Input
              {...control}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              inputMode="decimal"
            />
          )}
        </Field>
        <Field label={strings.crmFieldCurrency} hint={strings.crmCurrencyHint}>
          {(control) => (
            <Input
              {...control}
              value={currency}
              onChange={(e) => setCurrency(e.target.value)}
              maxLength={3}
              placeholder="EUR"
            />
          )}
        </Field>
      </div>

      <div className={styles.row}>
        <Field label={strings.crmFieldExpectedClose}>
          {(control) => (
            <Input
              {...control}
              type="date"
              value={expectedClose}
              onChange={(e) => setExpectedClose(e.target.value)}
            />
          )}
        </Field>
        <Field label={strings.crmFieldSource} hint={strings.crmSourceHint}>
          {(control) => (
            <Input
              {...control}
              value={source}
              onChange={(e) => setSource(e.target.value)}
            />
          )}
        </Field>
      </div>
    </DialogFrame>
  );
}
