// My claims — what I spent, where each claim stands, and the two verbs that
// move it: hand it in, or take it back.
//
// Every claim on this screen is the caller's own. There is no user id anywhere
// on this path and no way to ask for somebody else's: the account door binds the
// person on the server, and a receipt names a restaurant, a pharmacy or an
// occasion (`docs/design/finance.md` § Expenses are personal data). The one
// cross-person screen in the module is the approver's, and it is a different
// tab behind a different door.
//
// What a row offers is decided by the server's own `editable`, not by this
// file's reading of `status`: a claim in somebody's queue is frozen, and the
// way back is to withdraw it. Showing an Edit button that always fails would
// teach the rule by refusal, which is the one way a rule must never be taught.
import { useCallback, useEffect, useState } from "react";
import { Plus, ReceiptText } from "lucide-react";

import { quarterOf } from "../billing";
import {
  Button,
  Input,
  Select,
  Spinner,
  Table,
  Td,
  Th,
  Toolbar,
  ToolbarSpacer,
  useDialogs,
} from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { ExpenseDialog, type ProjectChoice } from "./ExpenseDialog";
import { amountLabel, dayLabel, methodLabel, statusLabel } from "./format";
import { EmptyState, ErrorBanner, StatusChip } from "./parts";
import type { Expense, ExpenseStatus } from "./types";
import styles from "./FinanceModule.module.css";

/** The states a claim can be filtered to, in the order the flow runs. */
const STATUSES: ExpenseStatus[] = [
  "draft",
  "submitted",
  "approved",
  "rejected",
  "reimbursed",
];

export function ExpensesView({
  projects,
  revision: outerRevision,
}: {
  projects: ProjectChoice[];
  /** Bumped by the module when a decision elsewhere changed one of these
   *  claims — an approval on the approver's tab is a state change on this
   *  screen, and a person should not have to reload to see it. */
  revision: number;
}) {
  const api = useFinanceApi();
  const dialogs = useDialogs();
  // The quarter is what a person is asked about ("did you claim that trip?"),
  // and it is the period Billing's VAT summary already opens on — read from
  // there rather than given a second definition that disagrees at a boundary.
  const [period, setPeriod] = useState(() => quarterOf(new Date()));
  const [form, setForm] = useState(period);
  const [status, setStatus] = useState<ExpenseStatus | "">("");
  const [claims, setClaims] = useState<Expense[]>([]);
  const [editing, setEditing] = useState<Expense | null>(null);
  const [creating, setCreating] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => setRevision((r) => r + 1), []);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const list = await api.expenses(
          period.from,
          period.to,
          status === "" ? undefined : status,
        );
        if (live) {
          setClaims(list);
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
  }, [api, period, status, revision, outerRevision]);

  /** One verb on one claim, with the server's own sentence when it refuses. */
  async function act(claim: Expense, verb: "submit" | "withdraw") {
    setBusy(claim.id);
    setError(null);
    try {
      if (verb === "submit") await api.submitExpense(claim.id);
      else await api.withdrawExpense(claim.id);
      reload();
    } catch (err) {
      setError(financeMessage(err, strings.financeSaveFailed));
    } finally {
      setBusy(null);
    }
  }

  /** Removes a claim. Confirmed rather than undoable: there is nothing on the
   *  server to put back, and a claim removed by accident is a receipt somebody
   *  has to find again. */
  async function remove(claim: Expense) {
    const ok = await dialogs.confirm({
      title: strings.financeDeleteTitle,
      message: strings.financeDeleteBody,
      confirmLabel: strings.financeDelete,
      danger: true,
    });
    if (!ok) return;
    setBusy(claim.id);
    setError(null);
    try {
      await api.deleteExpense(claim.id);
      setEditing(null);
      reload();
    } catch (err) {
      setError(financeMessage(err, strings.financeSaveFailed));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className={styles.page}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          setPeriod(form);
        }}
      >
        <Toolbar label={strings.financeClaimFilters}>
          <label className={styles.periodField}>
            {strings.financeFrom}
            <Input
              type="date"
              value={form.from}
              onChange={(e) => setForm({ ...form, from: e.target.value })}
              required
            />
          </label>
          <label className={styles.periodField}>
            {strings.financeTo}
            <Input
              type="date"
              value={form.to}
              onChange={(e) => setForm({ ...form, to: e.target.value })}
              required
            />
          </label>
          <Button type="submit" variant="ghost" size="sm">
            {strings.financeShow}
          </Button>
          <label className={styles.periodField}>
            {strings.financeStatus}
            {/* "Any state" is an answer somebody must be able to return to,
                not a prompt, so it stays choosable — which is the whole of
                `Select`'s `placeholder` distinction. */}
            <Select
              value={status}
              placeholder={strings.financeAnyStatus}
              onChange={(e) => setStatus(e.target.value as ExpenseStatus | "")}
            >
              {STATUSES.map((state) => (
                <option key={state} value={state}>
                  {statusLabel(state)}
                </option>
              ))}
            </Select>
          </label>
          <ToolbarSpacer />
          {loading && <Spinner size={16} />}
          <Button onClick={() => setCreating(true)}>
            <Plus size={16} /> {strings.financeNewClaim}
          </Button>
        </Toolbar>
      </form>

      {error !== null && <ErrorBanner message={error} />}

      {claims.length === 0 && !loading ? (
        <EmptyState
          Icon={ReceiptText}
          title={strings.financeExpensesEmptyTitle}
          body={strings.financeExpensesEmptyBody}
          cta={strings.financeNewClaim}
          onCta={() => setCreating(true)}
        />
      ) : (
        <Table label={strings.financeClaimsTable}>
          <thead>
            <tr>
              <Th>{strings.financeSpentOn}</Th>
              <Th>{strings.financeMerchant}</Th>
              <Th>{strings.financeMethod}</Th>
              <Th numeric>{strings.financeGross}</Th>
              <Th numeric>{strings.financeVat}</Th>
              <Th>{strings.financeStatus}</Th>
              {/* Present for a screen reader, not on screen: a nameless column
                  of buttons is announced as nothing at all. */}
              <Th hideLabel>{strings.financeActions}</Th>
            </tr>
          </thead>
          <tbody>
            {claims.map((claim) => (
              <tr key={claim.id}>
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
                  {/* What the approver wrote is the whole point of a refusal:
                      the person is meant to read it, fix the claim and hand
                      it in again. */}
                  {claim.status === "rejected" && claim.decisionNote !== "" && (
                    <span className={styles.declined}>
                      {claim.decisionNote}
                    </span>
                  )}
                </Td>
                <Td className={styles.muted}>{methodLabel(claim.method)}</Td>
                <Td numeric>{amountLabel(claim.grossCents, claim.currency)}</Td>
                <Td numeric>
                  {claim.vatCents === 0 ? (
                    <span className={styles.muted}>{strings.financeNoVat}</span>
                  ) : (
                    amountLabel(claim.vatCents, claim.currency)
                  )}
                </Td>
                <Td>
                  <StatusChip status={claim.status} />
                  {claim.reimbursedOn !== null && (
                    <span className={styles.subtle}>
                      {strings.financePaidBackOn(dayLabel(claim.reimbursedOn))}
                    </span>
                  )}
                </Td>
                <Td>
                  <div className={styles.rowActions}>
                    {claim.editable && (
                      <>
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={busy !== null}
                          onClick={() => setEditing(claim)}
                        >
                          {strings.financeEdit}
                        </Button>
                        <Button
                          size="sm"
                          disabled={busy !== null}
                          onClick={() => void act(claim, "submit")}
                        >
                          {strings.financeSubmit}
                        </Button>
                      </>
                    )}
                    {claim.status === "submitted" && (
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={busy !== null}
                        onClick={() => void act(claim, "withdraw")}
                      >
                        {strings.financeWithdraw}
                      </Button>
                    )}
                  </div>
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}

      {(creating || editing !== null) && (
        <ExpenseDialog
          claim={editing}
          projects={projects}
          onClose={() => {
            setCreating(false);
            setEditing(null);
          }}
          onSaved={() => {
            setCreating(false);
            setEditing(null);
            reload();
          }}
          onDelete={editing === null ? undefined : () => void remove(editing)}
        />
      )}
    </div>
  );
}
