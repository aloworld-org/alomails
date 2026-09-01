import { useEffect, useMemo, useState } from "react";
import { ArrowRight, BanknoteArrowDown, CircleAlert, Landmark, ReceiptText } from "lucide-react";
import { Link } from "react-router-dom";

import { Card, Spinner } from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { amountLabel, today } from "./format";
import { ErrorBanner } from "./parts";
import type { AgedReport, BankLine, BankStatement, PendingExpense } from "./types";

interface OverviewData {
  pending: PendingExpense[];
  reimbursable: PendingExpense[];
  unmatched: BankLine[];
  statements: BankStatement[];
  receivables: AgedReport;
}

export function FinanceOverviewView() {
  const api = useFinanceApi();
  const [data, setData] = useState<OverviewData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    const on = today();
    void Promise.all([
      api.pendingExpenses(),
      api.reimbursableExpenses(),
      api.bankLines({ status: "unmatched" }),
      api.bankStatements(),
      api.agedReport(on, "receivable"),
    ]).then(([pending, reimbursable, unmatched, statements, receivables]) => {
      if (live) setData({ pending, reimbursable, unmatched, statements, receivables });
    }).catch((reason: unknown) => {
      if (live) setError(financeMessage(reason, strings.financeOverviewLoadFailed));
    });
    return () => { live = false; };
  }, [api]);

  const attention = useMemo(() => data === null ? 0 : data.pending.length + data.reimbursable.length + data.unmatched.length, [data]);

  if (data === null && error === null) return <div className="grid flex-1 place-items-center"><Spinner size={22} /></div>;

  return (
    <main className="min-h-0 flex-1 overflow-auto px-8 py-6 max-sm:px-4">
      <div className="mx-auto flex w-full max-w-[108rem] flex-col gap-5">
        {error !== null && <ErrorBanner message={error} />}
        {data !== null && <>
          <section className="flex flex-wrap items-end justify-between gap-3">
            <div>
              <p className="m-0 text-xs font-semibold uppercase tracking-[0.12em] text-accent">{strings.financeOverviewEyebrow}</p>
              <h2 className="m-0 mt-1 text-xl font-semibold tracking-tight text-primary">{strings.financeOverviewTitle}</h2>
              <p className="m-0 mt-1 text-sm text-secondary">{strings.financeOverviewSubtitle}</p>
            </div>
            <span className="rounded-full bg-[var(--accent-soft)] px-3 py-1.5 text-sm font-semibold text-accent">{strings.financeAttentionCount(attention)}</span>
          </section>

          <section className="grid gap-3 md:grid-cols-2 xl:grid-cols-4" aria-label={strings.financeOverviewTitle}>
            <Metric Icon={CircleAlert} label={strings.financePendingApprovals} value={String(data.pending.length)} detail={strings.financeNeedsDecision} to="/finance/approvals" />
            <Metric Icon={BanknoteArrowDown} label={strings.financeToReimburse} value={String(data.reimbursable.length)} detail={strings.financeReadyToPay} to="/finance/approvals" />
            <Metric Icon={Landmark} label={strings.financeUnreconciled} value={String(data.unmatched.length)} detail={strings.financeBankItems} to="/finance/reconcile" />
            <Metric Icon={ReceiptText} label={strings.financeReceivables} value={amountLabel(data.receivables.buckets.totalCents, data.receivables.currency)} detail={strings.financeOpenDocuments(data.receivables.documentCount)} to="/finance/reports/aged-receivable" />
          </section>

          <section className="grid gap-5 xl:grid-cols-[minmax(0,1.35fr)_minmax(20rem,0.65fr)]">
            <Card as="section" pad="none" className="overflow-hidden">
              <div className="border-b border-subtle px-5 py-4"><h3 className="m-0 text-base font-semibold text-primary">{strings.financeNeedsAttention}</h3><p className="m-0 mt-1 text-sm text-secondary">{strings.financeNeedsAttentionHint}</p></div>
              <div className="divide-y divide-subtle">
                <AttentionRow Icon={CircleAlert} title={strings.financePendingApprovals} count={data.pending.length} to="/finance/approvals" />
                <AttentionRow Icon={BanknoteArrowDown} title={strings.financeToReimburse} count={data.reimbursable.length} to="/finance/approvals" />
                <AttentionRow Icon={Landmark} title={strings.financeUnreconciled} count={data.unmatched.length} to="/finance/reconcile" />
              </div>
            </Card>
            <Card as="section" pad="md">
              <div className="flex items-start justify-between gap-3"><div><p className="m-0 text-xs font-semibold uppercase tracking-[0.12em] text-secondary">{strings.financeBanking}</p><h3 className="m-0 mt-1 text-base font-semibold text-primary">{strings.financeLatestStatement}</h3></div><span className="grid size-10 place-items-center rounded-xl bg-[var(--accent-soft)] text-accent"><Landmark className="size-5" /></span></div>
              {data.statements[0] === undefined ? <p className="mb-0 mt-5 text-sm text-secondary">{strings.financeNoStatements}</p> : <div className="mt-5"><p className="m-0 font-semibold text-primary">{data.statements[0].accountIban}</p><p className="m-0 mt-1 text-sm text-secondary">{strings.financeStatementLines(data.statements[0].lineCount)}</p><p className="m-0 mt-4 text-2xl font-semibold tracking-tight text-primary">{data.statements[0].closingBalanceCents === null ? "—" : amountLabel(data.statements[0].closingBalanceCents, data.statements[0].currency)}</p><p className="m-0 mt-1 text-xs text-secondary">{strings.financeClosingBalance}</p></div>}
              <Link className="mt-5 inline-flex items-center gap-1.5 text-sm font-semibold text-accent hover:underline" to="/finance/bank">{strings.financeOpenBanking}<ArrowRight className="size-4" /></Link>
            </Card>
          </section>
        </>}
      </div>
    </main>
  );
}

function Metric({ Icon, label, value, detail, to }: { Icon: typeof CircleAlert; label: string; value: string; detail: string; to: string }) {
  return <Link to={to} className="rounded-2xl focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"><Card interactive pad="sm" className="h-full"><div className="flex items-start justify-between gap-3"><span className="grid size-10 place-items-center rounded-xl bg-[var(--accent-soft)] text-accent"><Icon className="size-5" /></span><ArrowRight className="size-4 text-tertiary" /></div><p className="m-0 mt-4 text-2xl font-semibold tracking-tight text-primary">{value}</p><p className="m-0 mt-1 text-sm font-medium text-primary">{label}</p><p className="m-0 mt-1 text-xs text-secondary">{detail}</p></Card></Link>;
}

function AttentionRow({ Icon, title, count, to }: { Icon: typeof CircleAlert; title: string; count: number; to: string }) {
  return <Link to={to} className="flex items-center gap-3 px-5 py-4 transition-colors hover:bg-raised"><span className="grid size-9 place-items-center rounded-xl bg-[var(--accent-soft)] text-accent"><Icon className="size-4" /></span><span className="min-w-0 flex-1 text-sm font-medium text-primary">{title}</span><span className="rounded-full bg-raised px-2.5 py-1 text-xs font-semibold text-primary">{count}</span><ArrowRight className="size-4 text-tertiary" /></Link>;
}
