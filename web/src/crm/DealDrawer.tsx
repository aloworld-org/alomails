// A deal opens in a drawer, not a page (`docs/design/crm.md` § Web surface):
// value and stage at the top, then the log, what happens next, and the
// conversations it belongs to. It slides over the board so the card the user
// clicked is still where they left it.
//
// The drawer re-reads the deal itself rather than being handed the row the
// board is holding: a move, an edit or a colleague's change must show the
// STORED record, which is the same contract every CRM write holds.
import { useCallback, useEffect, useState } from "react";
import { Pencil, Trash2, X } from "lucide-react";

import { useDialogs } from "../ds";
import { strings } from "../i18n";
import { ActivityLog } from "./ActivityLog";
import { crmMessage, useCrmApi } from "./api";
import { DealDialog } from "./DealDialog";
import { dayLabel, dealValue } from "./format";
import { LinkedThreads } from "./LinkedThreads";
import { moveDeal } from "./moveDeal";
import { NextSteps } from "./NextSteps";
import { ErrorBanner, StateChip } from "./parts";
import type { CrmDeal, CrmStage } from "./types";
import styles from "./CrmModule.module.css";

interface Props {
  dealId: string;
  /** The columns of the deal's board — a deal may only be moved within it. */
  stages: CrmStage[];
  onClose: () => void;
  /** Something about the deal changed: the board or list behind re-reads. */
  onChanged: () => void;
}

export function DealDrawer({ dealId, stages, onClose, onChanged }: Props) {
  const api = useCrmApi();
  const dialogs = useDialogs();
  const [deal, setDeal] = useState<CrmDeal | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);

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
      const moved = await moveDeal(api, dialogs, dealId, stage);
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
    <aside className={styles.drawer} role="dialog" aria-modal="false" aria-label={strings.crmDeal}>
      <header className={styles.drawerHead}>
        <div className={styles.drawerTitleRow}>
          <h2 className={styles.drawerTitle}>{deal?.title ?? strings.crmDeal}</h2>
          <button
            type="button"
            className={styles.iconAction}
            onClick={onClose}
            aria-label={strings.crmClose}
          >
            <X size={18} />
          </button>
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
              <p className={styles.drawerLost}>{strings.crmLostBecause(deal.lostReason)}</p>
            )}
            <div className={styles.drawerActions}>
              <label className={styles.stagePicker}>
                <span className={styles.label}>{strings.crmStage}</span>
                <select
                  className={styles.filterSelect}
                  value={deal.stageId}
                  onChange={(e) => void move(e.target.value)}
                >
                  {/* A closed deal can sit in a column that has since been
                      archived. Say so rather than let the select fall back to
                      its first option, which would show the wrong column and
                      turn an idle click into a move. */}
                  {!stages.some((s) => s.id === deal.stageId) && (
                    <option value={deal.stageId}>{strings.crmStageArchived}</option>
                  )}
                  {stages.map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.name}
                    </option>
                  ))}
                </select>
              </label>
              <span className={styles.cardSpacer} />
              <button type="button" className={styles.linkAction} onClick={() => setEditing(true)}>
                <Pencil size={13} /> {strings.crmEdit}
              </button>
              <button type="button" className={styles.linkAction} onClick={() => void remove()}>
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
    </aside>
  );
}
