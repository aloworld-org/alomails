// A deal opens in a drawer, not a page (`docs/design/crm.md` § Web surface):
// value and stage at the top, then the log, what happens next, and the
// conversations it belongs to. It slides over the board so the card the user
// clicked is still where they left it.
//
// The drawer re-reads the deal itself rather than being handed the row the
// board is holding: a move, an edit or a colleague's change must show the
// STORED record, which is the same contract every CRM write holds.
import { useCallback, useEffect, useState } from "react";
import {
  BriefcaseBusiness,
  ExternalLink,
  FileText,
  Pencil,
  Receipt,
  Trash2,
  X,
} from "lucide-react";
import { useNavigate } from "react-router-dom";

import { RecordAgentPanel, type RecordOrigin } from "../agents";
import { RecordHistory } from "../audit";
import { Button, IconButton, Modal, useDialogs } from "../ds";
import { strings } from "../i18n";
import { ActivityLog } from "./ActivityLog";
import { crmMessage, useCrmApi } from "./api";
import { DealDialog } from "./DealDialog";
import { dayLabel, dealValue } from "./format";
import { LinkedThreads } from "./LinkedThreads";
import { useLostReason } from "./LostReasonDialog";
import { moveDeal } from "./moveDeal";
import { NextSteps } from "./NextSteps";
import { ErrorBanner, StateChip } from "./parts";
import { RaiseDocumentDialog } from "./RaiseDocumentDialog";
import { RelatedBillingDocuments } from "./RelatedBillingDocuments";
import type { CrmDeal, CrmStage, DealProject, DocumentKind } from "./types";
import { WonDealProjectDialog } from "./WonDealProjectDialog";

interface Props {
  dealId: string;
  /** The columns of the deal's board — a deal may only be moved within it. */
  stages: CrmStage[];
  onClose: () => void;
  /** Something about the deal changed: the board or list behind re-reads. */
  onChanged: () => void;
}

/** Where this deal came from, in the provenance shape the agent panel
 *  renders. A deal's record carries its source as the free words the person
 *  or import gave it ("Referral", "Website"), so those words are the
 *  citation. `createdBy` is deliberately not a fallback: it holds the
 *  creator's opaque subject id, and an origin is said in words or not at
 *  all — the readable join is A4.5's, adopted when the deal read carries
 *  it. */
export function dealOrigin(deal: CrmDeal): RecordOrigin | null {
  if (deal.source !== "") {
    return { kind: "source", id: deal.source, label: deal.source };
  }
  return null;
}

export function DealDrawer({ dealId, stages, onClose, onChanged }: Props) {
  const api = useCrmApi();
  const dialogs = useDialogs();
  const lost = useLostReason();
  const navigate = useNavigate();
  const [deal, setDeal] = useState<CrmDeal | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  /** `null` = closed; a kind = raising that document from this deal (B2.08). */
  const [raising, setRaising] = useState<DocumentKind | null>(null);
  const [project, setProject] = useState<DealProject | null>(null);
  const [creatingProject, setCreatingProject] = useState(false);
  const [documentRevision, setDocumentRevision] = useState(0);

  const load = useCallback(async () => {
    try {
      const [storedDeal, linkedProject] = await Promise.all([
        api.deal(dealId),
        api.dealProject(dealId),
      ]);
      setDeal(storedDeal);
      setProject(linkedProject);
      setError(null);
    } catch (err) {
      setError(crmMessage(err, strings.crmLoadFailed));
    }
  }, [api, dealId]);

  useEffect(() => {
    void load();
  }, [load]);

  /** Moving from the drawer is the same move the board makes — the same
   *  question about a losing column, the same single transaction. */
  async function move(stageId: string) {
    const stage = stages.find((s) => s.id === stageId);
    if (stage === undefined) return;
    try {
      const moved = await moveDeal(api, lost.ask, dealId, stage);
      if (moved === null) return;
      setDeal(moved);
      onChanged();
    } catch (err) {
      setError(crmMessage(err, strings.crmSaveFailed));
    }
  }

  async function remove() {
    if (
      !(await dialogs.confirm({
        title: strings.crmDeleteDeal,
        message: strings.crmDeleteDealConfirm,
        confirmLabel: strings.crmDeleteDeal,
        danger: true,
      }))
    ) {
      return;
    }
    try {
      await api.deleteDeal(dealId);
      onChanged();
      onClose();
    } catch (err) {
      setError(crmMessage(err, strings.crmDeleteFailed));
    }
  }

  return (
    <Modal
      title={deal?.title ?? strings.crmDeal}
      onClose={onClose}
      wide="extra"
      tall="page"
      icon={<BriefcaseBusiness size={18} />}
      actions={
        <IconButton
          label={strings.crmClose}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
    >
      <header className="shrink-0 border-b border-subtle bg-raised/35 px-6 py-5">
        {deal !== null && (
          <>
            <div className="flex flex-wrap items-center gap-3">
              <span className="text-2xl font-semibold tabular-nums text-primary">{dealValue(deal)}</span>
              <StateChip state={deal.state} />
              {deal.expectedClose !== null && (
                <span className="text-sm text-secondary">
                  {strings.crmExpectedClose(dayLabel(deal.expectedClose))}
                </span>
              )}
            </div>
            {deal.companyName !== "" && (
              <p className="mb-0 mt-2 text-sm text-secondary">
                {[deal.companyName, deal.contactName, deal.contactEmail]
                  .filter((v) => v !== "")
                  .join(" · ")}
              </p>
            )}
            {deal.lostReason !== null && (
              <p className="mb-0 mt-2 text-sm text-danger">
                {strings.crmLostBecause(deal.lostReason)}
              </p>
            )}
            <div className="mt-5 flex flex-wrap items-end gap-3">
              <fieldset className="w-full">
                <legend className="mb-2 text-sm font-semibold text-primary">{strings.crmStage}</legend>
                <div className="flex flex-wrap gap-2">
                  {!stages.some((stage) => stage.id === deal.stageId) && (
                    <span className="inline-flex min-h-10 items-center rounded-lg border border-default bg-raised px-4 text-sm font-medium text-secondary">
                      {strings.crmStageArchived}
                    </span>
                  )}
                  {stages.map((stage) => {
                    const selected = stage.id === deal.stageId;
                    const dot = stage.isWon ? "bg-success" : stage.isLost ? "bg-danger" : "bg-accent";
                    return (
                      <label key={stage.id} className="cursor-pointer">
                        <input
                          className="peer sr-only"
                          type="radio"
                          name={`deal-stage-${deal.id}`}
                          value={stage.id}
                          checked={selected}
                          onChange={() => void move(stage.id)}
                        />
                        <span className="inline-flex min-h-10 items-center gap-2 rounded-lg border border-default bg-surface px-4 text-sm font-medium text-secondary transition-colors hover:border-accent/40 hover:bg-raised hover:text-primary peer-checked:border-accent peer-checked:bg-accent-soft peer-checked:text-accent peer-checked:shadow-sm peer-focus-visible:outline-2 peer-focus-visible:outline-accent peer-focus-visible:outline-offset-2">
                          <span className={`size-2 rounded-full ${dot}`} aria-hidden="true" />
                          {stage.name}
                        </span>
                      </label>
                    );
                  })}
                </div>
              </fieldset>
              <div className="ml-auto flex flex-wrap items-center justify-end gap-1.5">
                {/* The handoff to billing (B2.08). Offered on any deal that has
                    not been lost, because quoting an open deal is how it is won —
                    and both raise a DRAFT the tenant then edits in billing. */}
                {deal.state !== "lost" && (
                  <>
                    <Button variant="primary" icon={<FileText />} onClick={() => setRaising("quote")}>{strings.crmRaiseQuote}</Button>
                    <Button className="!px-3" variant="ghost" icon={<Receipt />} onClick={() => setRaising("invoice")}>{strings.crmRaiseInvoice}</Button>
                  </>
                )}
                {deal.state === "won" && project === null && (
                  <Button className="!px-3" variant="ghost" icon={<BriefcaseBusiness />} onClick={() => setCreatingProject(true)}>{strings.crmCreateProject}</Button>
                )}
                <Button className="!px-3" variant="ghost" icon={<Pencil />} onClick={() => setEditing(true)}>{strings.crmEdit}</Button>
                <button
                  type="button"
                  className="inline-flex min-h-10 shrink-0 items-center gap-2 rounded-lg !px-3 text-sm font-medium text-danger transition-colors hover:bg-danger/10 focus-visible:outline-2 focus-visible:outline-danger"
                  onClick={() => void remove()}
                >
                  <Trash2 size={16} /> {strings.crmDeleteDeal}
                </button>
              </div>
            </div>
          </>
        )}
      </header>

      {error !== null && <div className="shrink-0 px-6 pt-4"><ErrorBanner message={error} /></div>}

      {deal !== null && (
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1.45fr)_minmax(18rem,.75fr)] items-start gap-5 overflow-y-auto p-6 max-lg:grid-cols-1">
          <main className="flex min-w-0 flex-col gap-5">
            <NextSteps dealId={deal.id} />
            <ActivityLog dealId={deal.id} />
            <LinkedThreads dealId={deal.id} />
          </main>
          <aside className="flex min-w-0 flex-col gap-5">
            {project !== null && (
              <section className="rounded-xl border border-subtle bg-surface p-4 shadow-sm">
              <div className="flex items-start gap-3">
                <span className="rounded-lg bg-accent-soft p-2 text-accent">
                  <BriefcaseBusiness size={18} />
                </span>
                <div className="min-w-0 flex-1">
                  <p className="m-0 text-xs font-semibold uppercase tracking-wide text-tertiary">
                    {strings.crmDeliveryProject}
                  </p>
                  <p className="mt-1 truncate font-semibold text-primary">
                    {project.projectName}
                  </p>
                </div>
                <IconButton
                  label={strings.crmOpenProject}
                  icon={<ExternalLink size={17} />}
                  onClick={() => {
                    onClose();
                    navigate(`/projects/${encodeURIComponent(project.projectId)}/overview`);
                  }}
                />
              </div>
              </section>
            )}
            <RelatedBillingDocuments dealId={deal.id} revision={documentRevision} />
            <RecordAgentPanel product="crm" recordKind="deal" recordId={deal.id} recordLabel={deal.title} origin={dealOrigin(deal)} onBeforeNavigate={onClose} />
          {/* Who changed this deal, and when (B2.13). Last in the drawer: it is
              the question asked after the ones above, never instead of them. */}
            <section className="rounded-xl border border-subtle bg-surface p-4 shadow-sm">
              <RecordHistory entityType="crm.deal" entityId={deal.id} />
            </section>
          </aside>
        </div>
      )}

      {editing && deal !== null && (
        <DealDialog
          deal={deal}
          pipelineId={deal.pipelineId}
          stageId={deal.stageId}
          onClose={() => setEditing(false)}
          onSaved={(saved) => {
            setDeal(saved);
            setEditing(false);
            onChanged();
          }}
        />
      )}

      {raising !== null && deal !== null && (
        <RaiseDocumentDialog
          deal={deal}
          kind={raising}
          onClose={() => setRaising(null)}
          onRaised={(raised) => {
            // Raising a document can give a lead a customer, so the drawer
            // redraws from the server's answer rather than from what it held.
            setDeal(raised);
            setDocumentRevision((current) => current + 1);
            onChanged();
          }}
        />
      )}

      {creatingProject && deal !== null && (
        <WonDealProjectDialog
          deal={deal}
          onClose={() => setCreatingProject(false)}
          onCreated={(created) => {
            setProject(created);
            setCreatingProject(false);
            onChanged();
          }}
        />
      )}

      {lost.dialog}
    </Modal>
  );
}
