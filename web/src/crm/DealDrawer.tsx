// A deal opens in a drawer, not a page (`docs/design/crm.md` § Web surface):
// value and stage at the top, then the log, what happens next, and the
// conversations it belongs to. It slides over the board so the card the user
// clicked is still where they left it.
//
// The drawer re-reads the deal itself rather than being handed the row the
// board is holding: a move, an edit or a colleague's change must show the
// STORED record, which is the same contract every CRM write holds.
import { useCallback, useEffect, useState } from "react";
import { FileText, Pencil, Receipt, Trash2, X } from "lucide-react";

import { RecordAgentPanel, type RecordOrigin } from "../agents";
import { RecordHistory } from "../audit";
import { Field, IconButton, Select, useDialogs } from "../ds";
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
import type { CrmDeal, CrmStage, DocumentKind } from "./types";
import styles from "./CrmModule.module.css";

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
  const [deal, setDeal] = useState<CrmDeal | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  /** `null` = closed; a kind = raising that document from this deal (B2.08). */
  const [raising, setRaising] = useState<DocumentKind | null>(null);

  const load = useCallback(async () => {
    try {
      setDeal(await api.deal(dealId));
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
    <aside
      className={styles.drawer}
      role="dialog"
      aria-modal="false"
      aria-label={strings.crmDeal}
    >
      <header className={styles.drawerHead}>
        <div className={styles.drawerTitleRow}>
          <h2 className={styles.drawerTitle}>
            {deal?.title ?? strings.crmDeal}
          </h2>
          <IconButton
            label={strings.crmClose}
            icon={<X size={18} />}
            onClick={onClose}
          />
        </div>
        {deal !== null && (
          <>
            <div className={styles.drawerFacts}>
              <span className={styles.drawerValue}>{dealValue(deal)}</span>
              <StateChip state={deal.state} />
              {deal.expectedClose !== null && (
                <span className={styles.drawerFact}>
                  {strings.crmExpectedClose(dayLabel(deal.expectedClose))}
                </span>
              )}
            </div>
            {deal.companyName !== "" && (
              <p className={styles.drawerCompany}>
                {[deal.companyName, deal.contactName, deal.contactEmail]
                  .filter((v) => v !== "")
                  .join(" · ")}
              </p>
            )}
            {deal.lostReason !== null && (
              <p className={styles.drawerLost}>
                {strings.crmLostBecause(deal.lostReason)}
              </p>
            )}
            <div className={styles.drawerActions}>
              <Field label={strings.crmStage}>
                {(control) => (
                  <Select
                    {...control}
                    value={deal.stageId}
                    onChange={(e) => void move(e.target.value)}
                  >
                    {/* A closed deal can sit in a column that has since been
                        archived. Say so rather than let the select fall back to
                        its first option, which would show the wrong column and
                        turn an idle click into a move. */}
                    {!stages.some((s) => s.id === deal.stageId) && (
                      <option value={deal.stageId}>
                        {strings.crmStageArchived}
                      </option>
                    )}
                    {stages.map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.name}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
              <span className={styles.cardSpacer} />
              {/* The handoff to billing (B2.08). Offered on any deal that has
                  not been lost, because quoting an open deal is how it is won —
                  and both raise a DRAFT the tenant then edits in billing. */}
              {deal.state !== "lost" && (
                <>
                  <button
                    type="button"
                    className={styles.linkAction}
                    onClick={() => setRaising("quote")}
                  >
                    <FileText size={13} /> {strings.crmRaiseQuote}
                  </button>
                  <button
                    type="button"
                    className={styles.linkAction}
                    onClick={() => setRaising("invoice")}
                  >
                    <Receipt size={13} /> {strings.crmRaiseInvoice}
                  </button>
                </>
              )}
              <button
                type="button"
                className={styles.linkAction}
                onClick={() => setEditing(true)}
              >
                <Pencil size={13} /> {strings.crmEdit}
              </button>
              <button
                type="button"
                className={styles.linkAction}
                onClick={() => void remove()}
              >
                <Trash2 size={13} /> {strings.crmDeleteDeal}
              </button>
            </div>
          </>
        )}
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {deal !== null && (
        <div className={styles.drawerBody}>
          <ActivityLog dealId={deal.id} />
          <NextSteps dealId={deal.id} />
          <LinkedThreads dealId={deal.id} />
          <RecordAgentPanel
            product="crm"
            recordKind="deal"
            recordId={deal.id}
            recordLabel={deal.title}
            origin={dealOrigin(deal)}
            onBeforeNavigate={onClose}
          />
          {/* Who changed this deal, and when (B2.13). Last in the drawer: it is
              the question asked after the ones above, never instead of them. */}
          <RecordHistory entityType="crm.deal" entityId={deal.id} />
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
            onChanged();
          }}
        />
      )}

      {lost.dialog}
    </aside>
  );
}
