import { useEffect, useState } from "react";
import { Handshake } from "lucide-react";

import { hundredthsToInput, parseHundredths } from "../billing";
import { Field, Input, Select } from "../ds";
import { strings } from "../i18n";
import { crmMessage, useCrmApi } from "./api";
import { DialogFrame } from "./parts";
import type { CrmDeal, CrmPipeline, CrmStage } from "./types";

interface Props {
  threadId: string;
  subject: string;
  senderName: string;
  senderEmail: string;
  onClose: () => void;
  onCreated: (deal: CrmDeal) => void;
}

/** One reviewable Mail → Sales handoff. The source thread is not an editable
 * field: it is the conversation under the dialog and is linked atomically. */
export function CreateOpportunityFromMailDialog({
  threadId,
  subject,
  senderName,
  senderEmail,
  onClose,
  onCreated,
}: Props) {
  const api = useCrmApi();
  const [pipelines, setPipelines] = useState<CrmPipeline[]>([]);
  const [stages, setStages] = useState<CrmStage[]>([]);
  const [pipelineId, setPipelineId] = useState("");
  const [stageId, setStageId] = useState("");
  const [title, setTitle] = useState(subject);
  const [company, setCompany] = useState("");
  const [contactName, setContactName] = useState(senderName);
  const [contactEmail, setContactEmail] = useState(senderEmail);
  const [value, setValue] = useState("");
  const [currency, setCurrency] = useState("EUR");
  const [expectedClose, setExpectedClose] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void api
      .pipelines()
      .then((available) => {
        if (!live) return;
        const active = available.filter((pipeline) => !pipeline.archived);
        setPipelines(active);
        if (active.length === 1) setPipelineId(active[0]?.id ?? "");
      })
      .catch((reason) => {
        if (live) setError(crmMessage(reason, strings.crmMailOpportunityLoadFailed));
      });
    return () => {
      live = false;
    };
  }, [api]);

  useEffect(() => {
    if (pipelineId === "") {
      setStages([]);
      setStageId("");
      return;
    }
    let live = true;
    setStageId("");
    void api
      .stages(pipelineId)
      .then((available) => {
        if (!live) return;
        const open = available.filter((stage) => !stage.archived && !stage.closed);
        setStages(open);
        if (open.length > 0) setStageId(open[0]?.id ?? "");
      })
      .catch((reason) => {
        if (live) setError(crmMessage(reason, strings.crmMailOpportunityLoadFailed));
      });
    return () => {
      live = false;
    };
  }, [api, pipelineId]);

  const valueCents = value.trim() === "" ? 0 : parseHundredths(value);
  const valueError = value.trim() !== "" && valueCents === null;

  async function create() {
    if (valueCents === null) return;
    setBusy(true);
    setError(null);
    try {
      const deal = await api.createDealFromMail({
        pipelineId,
        stageId,
        threadId,
        title: title.trim(),
        companyName: company.trim(),
        contactName: contactName.trim(),
        contactEmail: contactEmail.trim(),
        valueCents,
        currency: currency.trim().toUpperCase(),
        expectedClose: expectedClose === "" ? null : expectedClose,
        source: strings.crmMailSource,
      });
      onCreated(deal);
    } catch (reason) {
      setError(crmMessage(reason, strings.crmMailOpportunityCreateFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Handshake}
      title={strings.crmMailOpportunityTitle}
      subtitle={strings.crmMailOpportunitySubtitle}
      error={error}
      busy={busy}
      canSubmit={
        title.trim() !== "" && pipelineId !== "" && stageId !== "" && !valueError
      }
      submitLabel={strings.crmMailOpportunityConfirm}
      onClose={onClose}
      onSubmit={() => void create()}
    >
      <div className="rounded-xl border border-subtle bg-secondary p-4">
        <p className="m-0 text-xs font-semibold uppercase tracking-wide text-tertiary">
          {strings.crmMailConversation}
        </p>
        <p className="mb-0 mt-1 truncate text-sm font-semibold text-primary">{subject}</p>
        <p className="mb-0 mt-1 truncate text-xs text-secondary">{senderEmail}</p>
      </div>

      <div className="grid grid-cols-2 gap-4 max-sm:grid-cols-1">
        <Field label={strings.crmPipeline}>
          {(control) => (
            <Select
              {...control}
              value={pipelineId}
              onChange={(event) => setPipelineId(event.target.value)}
            >
              <option value="">{strings.crmChoosePipeline}</option>
              {pipelines.map((pipeline) => (
                <option key={pipeline.id} value={pipeline.id}>
                  {pipeline.name}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <Field label={strings.crmStage}>
          {(control) => (
            <Select
              {...control}
              value={stageId}
              disabled={pipelineId === ""}
              onChange={(event) => setStageId(event.target.value)}
            >
              <option value="">{strings.crmChooseStage}</option>
              {stages.map((stage) => (
                <option key={stage.id} value={stage.id}>
                  {stage.name}
                </option>
              ))}
            </Select>
          )}
        </Field>
      </div>

      <Field label={strings.crmFieldTitle}>
        {(control) => (
          <Input
            {...control}
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            required
          />
        )}
      </Field>
      <div className="grid grid-cols-2 gap-4 max-sm:grid-cols-1">
        <Field label={strings.crmFieldCompany}>
          {(control) => <Input {...control} value={company} onChange={(event) => setCompany(event.target.value)} />}
        </Field>
        <Field label={strings.crmFieldContactName}>
          {(control) => <Input {...control} value={contactName} onChange={(event) => setContactName(event.target.value)} />}
        </Field>
      </div>
      <Field label={strings.crmFieldContactEmail}>
        {(control) => <Input {...control} type="email" value={contactEmail} onChange={(event) => setContactEmail(event.target.value)} />}
      </Field>
      <div className="grid grid-cols-3 gap-4 max-sm:grid-cols-1">
        <Field label={strings.crmFieldValue} error={valueError ? strings.crmNotAnAmount : undefined}>
          {(control) => <Input {...control} value={value} onChange={(event) => setValue(event.target.value)} inputMode="decimal" placeholder={hundredthsToInput(0)} />}
        </Field>
        <Field label={strings.crmFieldCurrency}>
          {(control) => <Input {...control} value={currency} onChange={(event) => setCurrency(event.target.value)} maxLength={3} />}
        </Field>
        <Field label={strings.crmFieldExpectedClose}>
          {(control) => <Input {...control} type="date" value={expectedClose} onChange={(event) => setExpectedClose(event.target.value)} />}
        </Field>
      </div>
    </DialogFrame>
  );
}
