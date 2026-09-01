// The approver's screen: what is waiting for a decision, and what the company
// has decided to pay and still owes.
//
// This is the one screen in Finance that names a person, and the only one behind
// the admin-or-accountant door (`docs/design/finance.md` § The accountant role).
// The tab is **hidden entirely** for anybody else rather than shown disabled — a
// control that exists only to refuse teaches nothing — and the route still
// exists, so a manager's bookmark works and a colleague who follows one gets the
// server's own `403` instead of a page pretending the queue is empty.
//
// Two lists, not two tabs, because they are one person's inbox and the second is
// short. They are two *reads*, though, and deliberately so: an approved claim a
// company card paid is approved and is not owed to anybody, so "approved" is not
// the same list as "to pay back" (the server's `/finance/expenses/reimbursable`).
//
// A rejection carries a note, and this screen asks for one although the server
// accepts an empty one: the person whose claim comes back is going to read it,
// and "rejected" with no sentence is an approver making somebody guess.
import { useCallback, useEffect, useState } from "react";
import { Banknote, Inbox } from "lucide-react";

import { RecordAgentPanel } from "../agents";
import { Button, Spinner, Table, Td, Th, useDialogs } from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { amountLabel, dayLabel, methodLabel, momentLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import { ReimburseDialog } from "./ReimburseDialog";
import type { PendingExpense } from "./types";
import styles from "./FinanceModule.module.css";

export function ApprovalsView({ onDecided }: { onDecided: () => void }) {
  const api = useFinanceApi();
  const dialogs = useDialogs();
  const [waiting, setWaiting] = useState<PendingExpense[]>([]);
  const [owed, setOwed] = useState<PendingExpense[]>([]);
  const [paying, setPaying] = useState<PendingExpense | null>(null);
  /** The waiting claim in focus — its agent panel opens under the queue.
   *  An id, not the record: a decided claim leaves the list and takes the
   *  panel with it. */
  const [focused, setFocused] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  const reload = useCallback(() => setRevision((r) => r + 1), []);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        // Both queues in one round trip's worth of waiting: a decision on the
        // first list adds to the second, so a screen that loaded them in
        // sequence would show a claim in neither for an instant.
        const [pending, payable] = await Promise.all([
          api.pendingExpenses(),
          api.reimbursableExpenses(),
        ]);
        if (live) {
          setWaiting(pending);
          setOwed(payable);
          setError(null);
        }
      } catch (err) {
        if (live) setError(financeMessage(err, strings.financeLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, revision]);

  /** Approves a claim. No confirmation: approving is the ordinary act this
   *  screen exists for, and the way back is the reopen the store refuses —
   *  which is why the refusal, not a dialog, is what stops a mistake being
   *  quietly undone. */
  async function approve(claim: PendingExpense) {
    setBusy(claim.id);
    setError(null);
    try {
      const outcome = await api.approveExpense(claim.id);
      setNotice(outcome.approval.complete ? null : strings.financeApprovalRecorded(outcome.approval.count, outcome.approval.required));
      reload();
      onDecided();
    } catch (err) {
      setError(financeMessage(err, strings.financeSaveFailed));
    } finally {
      setBusy(null);
    }
  }

  async function reject(claim: PendingExpense) {
    const note = await dialogs.prompt({
      title: strings.financeRejectTitle,
      message: strings.financeRejectBody(claim.userEmail),
      confirmLabel: strings.financeReject,
      placeholder: strings.financeRejectPlaceholder,
    });
    // `null` is a cancelled prompt — not an empty note.
    if (note === null) return;
    setBusy(claim.id);
    setError(null);
    try {
      await api.rejectExpense(claim.id, note);
      reload();
      onDecided();
    } catch (err) {
      setError(financeMessage(err, strings.financeSaveFailed));
    } finally {
      setBusy(null);
    }
  }

  if (loading && waiting.length === 0 && owed.length === 0) {
    return (
      <div className={styles.page}>
        <Spinner size={20} />
      </div>
    );
  }

  return (
    <div className={styles.page}>
      {error !== null && <ErrorBanner message={error} />}
      {notice !== null && <p className={styles.notice}>{notice}</p>}

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>{strings.financeWaitingTitle}</h2>
        {waiting.length === 0 ? (
          <EmptyState
            Icon={Inbox}
            title={strings.financeWaitingEmptyTitle}
            body={strings.financeWaitingEmptyBody}
          />
        ) : (
          <Table label={strings.financePendingClaimsTable}>
            <thead>
              <tr>
                <Th>{strings.financePerson}</Th>
                <Th>{strings.financeSpentOn}</Th>
                <Th>{strings.financeMerchant}</Th>
                <Th>{strings.financeCategory}</Th>
                <Th>{strings.financeMethod}</Th>
                <Th numeric>{strings.financeGross}</Th>
                <Th>{strings.financeSubmittedAt}</Th>
                <Th hideLabel>{strings.financeActions}</Th>
              </tr>
            </thead>
            <tbody>
              {waiting.map((claim) => (
                <tr
                  key={claim.id}
                  aria-current={claim.id === focused ? "true" : undefined}
                >
                  <Td>
                    <button
                      type="button"
                      className="cursor-pointer border-0 bg-transparent p-0 text-left text-sm font-medium text-accent hover:underline"
                      aria-expanded={claim.id === focused}
                      onClick={() =>
                        setFocused(claim.id === focused ? null : claim.id)
                      }
                    >
                      {claim.userEmail}
                    </button>
                  </Td>
                  <Td>{dayLabel(claim.spentOn)}</Td>
                  <Td>
                    {claim.merchant === "" ? (
                      <span className={styles.muted}>
                        {strings.financeNoMerchant}
                      </span>
                    ) : (
                      claim.merchant
                    )}
                    {claim.description !== "" && (
                      <span className={styles.subtle}>{claim.description}</span>
                    )}
                  </Td>
                  <Td className={styles.muted}>
                    {claim.categoryName ?? strings.financeUncategorised}
                  </Td>
                  <Td className={styles.muted}>{methodLabel(claim.method)}</Td>
                  <Td numeric>
                    {amountLabel(claim.grossCents, claim.currency)}
                    {claim.vatCents !== 0 && (
                      <span className={styles.subtle}>
                        {strings.financeOfWhichVat(
                          amountLabel(claim.vatCents, claim.currency),
                        )}
                      </span>
                    )}
                  </Td>
                  <Td className={styles.muted}>
                    {claim.submittedAt === null
                      ? ""
                      : momentLabel(claim.submittedAt)}
                    {claim.approvalRequired > 1 && <span className={styles.subtle}>{strings.financeApprovalProgress(claim.approvalCount, claim.approvalRequired)}</span>}
                  </Td>
                  <Td>
                    <div className={styles.rowActions}>
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={busy !== null}
                        onClick={() => void reject(claim)}
                      >
                        {strings.financeReject}
                      </Button>
                      <Button
                        size="sm"
                        disabled={busy !== null}
                        onClick={() => void approve(claim)}
                      >
                        {strings.financeApprove}
                      </Button>
                    </div>
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
        {waiting
          .filter((claim) => claim.id === focused)
          .map((claim) => (
            <RecordAgentPanel
              key={claim.id}
              product="finance"
              recordKind="approval"
              recordId={claim.id}
              recordLabel={
                claim.merchant === "" ? claim.description : claim.merchant
              }
              origin={{
                kind: "person",
                id: claim.userId,
                label: claim.userEmail,
              }}
            />
          ))}
      </section>

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>{strings.financeOwedTitle}</h2>
        <p className={styles.sectionNote}>{strings.financeOwedNote}</p>
        {owed.length === 0 ? (
          <EmptyState
            Icon={Banknote}
            title={strings.financeOwedEmptyTitle}
            body={strings.financeOwedEmptyBody}
          />
        ) : (
          <Table label={strings.financeOwedClaimsTable}>
            <thead>
              <tr>
                <Th>{strings.financePerson}</Th>
                <Th>{strings.financeSpentOn}</Th>
                <Th>{strings.financeMerchant}</Th>
                <Th numeric>{strings.financeGross}</Th>
                <Th>{strings.financeApprovedAt}</Th>
                <Th hideLabel>{strings.financeActions}</Th>
              </tr>
            </thead>
            <tbody>
              {owed.map((claim) => (
                <tr key={claim.id}>
                  <Td>{claim.userEmail}</Td>
                  <Td>{dayLabel(claim.spentOn)}</Td>
                  <Td>
                    {claim.merchant === "" ? (
                      <span className={styles.muted}>
                        {strings.financeNoMerchant}
                      </span>
                    ) : (
                      claim.merchant
                    )}
                    {claim.decisionNote !== "" && (
                      <span className={styles.subtle}>
                        {claim.decisionNote}
                      </span>
                    )}
                  </Td>
                  <Td numeric>
                    {amountLabel(claim.grossCents, claim.currency)}
                  </Td>
                  <Td className={styles.muted}>
                    {claim.decidedAt === null
                      ? ""
                      : momentLabel(claim.decidedAt)}
                  </Td>
                  <Td>
                    <div className={styles.rowActions}>
                      <Button
                        size="sm"
                        disabled={busy !== null}
                        onClick={() => setPaying(claim)}
                      >
                        {strings.financeMarkPaidBack}
                      </Button>
                    </div>
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </section>

      {paying !== null && (
        <ReimburseDialog
          claim={paying}
          onClose={() => setPaying(null)}
          onDone={() => {
            setPaying(null);
            reload();
            onDecided();
          }}
        />
      )}
    </div>
  );
}
