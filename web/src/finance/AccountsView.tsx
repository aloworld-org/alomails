// The Accounts tab: the chart of accounts, editable, with what each account
// moved over a period.
//
// **The chart is never empty on a first visit.** The server seeds a neutral
// EU-SME chart on the first read and says so (`seeded`), so this screen opens on
// twenty working accounts rather than on an "add your first account" button that
// nobody outside accountancy knows how to answer. The one thing it must not do
// is present them silently — hence the notice, which says where they came from
// and that they are the tenant's own to rename.
//
// **Five tables, not one list.** A chart is read by kind — what is owned, what
// is owed, the owners' stake, what is earned, what is spent — and a single
// alphabetical list of twenty codes is a list nobody can check. Each section
// carries its own movement total when a period is asked for.
//
// **Nothing here computes money.** Every balance is the journal's, folded by the
// server over the period in the toolbar; the browser adds nothing up, and the
// column is blank rather than zero when no period was asked for.
import { useCallback, useEffect, useState } from "react";
import { BookOpen, Plus } from "lucide-react";

import { yearOf, type Period } from "../billing";
import { Button, Spinner, cx } from "../ds";
import { strings } from "../i18n";
import { AccountDialog } from "./AccountDialog";
import { financeMessage, useFinanceApi } from "./api";
import { accountRoleLabel, accountTypeLabel, amountLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import type { AccountDraft, AccountType, Chart, ChartAccount } from "./types";
import styles from "./FinanceModule.module.css";

/** The five kinds in the order a chart is laid out — assets first, because that
 *  is the order every printed chart in Europe is read in. */
const SECTIONS: AccountType[] = ["asset", "liability", "equity", "income", "expense"];

/** What is being edited: an existing account, or the new one being added. */
type Editing = { account: ChartAccount | null };

export function AccountsView() {
  const api = useFinanceApi();
  const [period, setPeriod] = useState<Period>(() => yearOf(new Date()));
  const [form, setForm] = useState<Period>(period);
  const [includeInactive, setIncludeInactive] = useState(false);
  const [chart, setChart] = useState<Chart | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [saving, setSaving] = useState(false);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  const reload = useCallback(() => setRevision((r) => r + 1), []);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const read = await api.chart({ includeInactive, from: period.from, to: period.to });
        if (live) {
          setChart(read);
          setError(null);
        }
      } catch (err) {
        // The server's own sentence when it sent one — a `403` here says the
        // chart is the bookkeepers', which is a thing worth reading verbatim.
        if (live) setError(financeMessage(err, strings.financeChartLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, includeInactive, period, revision]);

  async function save(draft: AccountDraft) {
    if (editing === null) return;
    setSaving(true);
    try {
      if (editing.account === null) await api.createAccount(draft);
      else await api.updateAccount(editing.account.id, draft);
      setEditing(null);
      setDialogError(null);
      reload();
    } catch (err) {
      // Stays open, carrying the refusal: a taken code and a held role are both
      // things the person can fix in the form they are looking at.
      setDialogError(financeMessage(err, strings.financeAccountSaveFailed));
    } finally {
      setSaving(false);
    }
  }

  async function remove(account: ChartAccount) {
    setSaving(true);
    try {
      await api.deleteAccount(account.id);
      setEditing(null);
      setDialogError(null);
      reload();
    } catch (err) {
      setDialogError(financeMessage(err, strings.financeAccountDeleteFailed));
    } finally {
      setSaving(false);
    }
  }

  const accounts = chart?.accounts ?? [];
  // The unit the movements are in, stated by the server beside them. Never
  // assumed: a figure whose currency a screen guessed reads wrongly the day a
  // tenant keeps books in something other than euros.
  const currency = chart?.currency ?? null;

  return (
    <div className={styles.page}>
      <form
        className={styles.toolbar}
        onSubmit={(e) => {
          e.preventDefault();
          setPeriod(form);
        }}
      >
        <label className={styles.periodField}>
          {strings.financeReportFrom}
          <input
            className={styles.periodInput}
            type="date"
            value={form.from}
            onChange={(e) => setForm({ ...form, from: e.target.value })}
            required
          />
        </label>
        <label className={styles.periodField}>
          {strings.financeReportTo}
          <input
            className={styles.periodInput}
            type="date"
            value={form.to}
            onChange={(e) => setForm({ ...form, to: e.target.value })}
            required
          />
        </label>
        <Button type="submit" variant="ghost">
          {strings.financeReportShow}
        </Button>
        <label className={styles.periodField}>
          <input
            type="checkbox"
            checked={includeInactive}
            onChange={(e) => setIncludeInactive(e.target.checked)}
          />
          {strings.financeAccountShowRetired}
        </label>
        <span className={styles.toolbarSpacer} />
        {loading && <Spinner size={16} />}
        <Button
          onClick={() => {
            setDialogError(null);
            setEditing({ account: null });
          }}
        >
          <Plus size={16} /> {strings.financeAccountAdd}
        </Button>
      </form>

      {error !== null && <ErrorBanner message={error} />}

      {/* Where twenty accounts nobody typed came from, said once, on the read
          that wrote them. */}
      {chart?.seeded === true && <p className={styles.notice}>{strings.financeChartSeeded}</p>}

      {accounts.length === 0 && !loading ? (
        <EmptyState
          Icon={BookOpen}
          title={strings.financeChartEmptyTitle}
          body={strings.financeChartEmptyBody}
          cta={strings.financeAccountAdd}
          onCta={() => {
            setDialogError(null);
            setEditing({ account: null });
          }}
        />
      ) : (
        SECTIONS.map((kind) => {
          const rows = accounts.filter((account) => account.type === kind);
          if (rows.length === 0) return null;
          return (
            <section key={kind} className={styles.section}>
              <h2 className={styles.sectionTitle}>{accountTypeLabel(kind)}</h2>
              <div className={styles.tableWrap}>
                <table className={styles.table}>
                  <thead>
                    <tr>
                      <th scope="col">{strings.financeAccountCode}</th>
                      <th scope="col">{strings.financeAccountName}</th>
                      <th scope="col">{strings.financeAccountRole}</th>
                      <th scope="col" className={styles.numeric}>
                        {strings.financeAccountMovement}
                      </th>
                      <th scope="col" className={styles.numeric}>
                        {strings.financeAccountPostings}
                      </th>
                      <th scope="col">
                        <span className={styles.srOnly}>{strings.financeAccountEdit}</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((account) => (
                      <tr key={account.id} className={account.active ? undefined : styles.declined}>
                        <td>{account.code}</td>
                        <td>
                          {account.name}
                          {!account.active && (
                            <span className={styles.subtle}>{strings.financeAccountRetired}</span>
                          )}
                        </td>
                        <td className={styles.muted}>
                          {account.role === null ? (
                            <span className={styles.muted}>{strings.financeNoVat}</span>
                          ) : (
                            accountRoleLabel(account.role)
                          )}
                        </td>
                        <td className={styles.numeric}>
                          {account.balanceCents === null || currency === null ? (
                            <span className={styles.muted}>{strings.financeNoVat}</span>
                          ) : (
                            amountLabel(account.balanceCents, currency)
                          )}
                        </td>
                        <td className={cx(styles.numeric, styles.muted)}>
                          {account.postings ?? ""}
                        </td>
                        <td className={styles.rowActions}>
                          <button
                            type="button"
                            className={styles.linkAction}
                            onClick={() => {
                              setDialogError(null);
                              setEditing({ account });
                            }}
                          >
                            {strings.financeAccountEdit}
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>
          );
        })
      )}

      {editing !== null && (
        <AccountDialog
          account={editing.account}
          busy={saving}
          error={dialogError}
          onClose={() => {
            setEditing(null);
            setDialogError(null);
          }}
          onSave={(draft) => void save(draft)}
          onDelete={
            // Only where it can succeed: the server refuses a seeded account and
            // one that carries a posting, and a button whose only outcome is a
            // refusal advertises a door that does not open. Retiring, which is
            // what they actually want, is a field of the form itself.
            editing.account !== null &&
            !editing.account.system &&
            (editing.account.postings ?? 0) === 0
              ? () => void remove(editing.account as ChartAccount)
              : undefined
          }
        />
      )}
    </div>
  );
}
