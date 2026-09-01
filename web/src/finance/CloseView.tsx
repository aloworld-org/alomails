import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, ChevronRight, CircleAlert, LockKeyhole, LockOpen, Plus } from "lucide-react";
import { Link } from "react-router-dom";

import { Badge, Button, Card, Input, Spinner, useDialogs } from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { ErrorBanner } from "./parts";
import type { CloseCheck, CloseReadiness, FinPeriod } from "./types";

function today() { return new Date().toISOString().slice(0, 10); }

const CHECKS: Record<CloseCheck["key"], { label: () => string; href: string }> = {
  bankReconciliation: { label: () => strings.financeCloseBank, href: "/finance/reconcile" },
  expenseApprovals: { label: () => strings.financeCloseExpenses, href: "/finance/approvals" },
  balanceSheet: { label: () => strings.financeCloseBalance, href: "/finance/reports/balance" },
  receivableFx: { label: () => strings.financeCloseReceivableFx, href: "/finance/reports/aged" },
  payableFx: { label: () => strings.financeClosePayableFx, href: "/finance/reports/aged" },
};

export function CloseView() {
  const api = useFinanceApi();
  const dialogs = useDialogs();
  const [periods, setPeriods] = useState<FinPeriod[]>([]);
  const [lockDate, setLockDate] = useState<string | null>(null);
  const [readiness, setReadiness] = useState<CloseReadiness | null>(null);
  const [fromDate, setFromDate] = useState("");
  const [toDate, setToDate] = useState(today());
  const [busy, setBusy] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const answer = await api.periods();
      setPeriods(answer.periods);
      setLockDate(answer.lockDate);
      const active = [...answer.periods].reverse().find((period) => period.status === "open");
      const on = active?.toDate ?? answer.periods.at(-1)?.toDate ?? today();
      setReadiness(await api.closeReadiness(on));
    } catch (reason) { setError(financeMessage(reason, strings.financeCloseLoadFailed)); }
    finally { setLoading(false); }
  }, [api]);

  useEffect(() => { void load(); }, [load]);

  async function create() {
    if (!fromDate || !toDate) return;
    setBusy("create"); setError(null);
    try { await api.createPeriod(fromDate, toDate); setFromDate(""); await load(); }
    catch (reason) { setError(financeMessage(reason, strings.financeCloseCreateFailed)); }
    finally { setBusy(null); }
  }

  async function close(period: FinPeriod) {
    const ok = await dialogs.confirm({ title: strings.financeCloseConfirmTitle, message: strings.financeCloseConfirmMessage(period.toDate), confirmLabel: strings.financeClosePeriod });
    if (!ok) return;
    setBusy(period.id); setError(null);
    try { await api.closePeriod(period.id); await load(); }
    catch (reason) { setError(financeMessage(reason, strings.financeCloseActionFailed)); }
    finally { setBusy(null); }
  }

  async function reopen(period: FinPeriod) {
    const reason = await dialogs.prompt({ title: strings.financeReopenPeriod, message: strings.financeReopenReason, confirmLabel: strings.financeReopenPeriod });
    if (reason === null || reason.trim() === "") return;
    setBusy(period.id); setError(null);
    try { await api.reopenPeriod(period.id, reason.trim()); await load(); }
    catch (failure) { setError(financeMessage(failure, strings.financeCloseActionFailed)); }
    finally { setBusy(null); }
  }

  if (loading) return <div className="grid flex-1 place-items-center"><Spinner size={22} /></div>;
  return <main className="min-h-0 flex-1 overflow-auto px-8 py-6 max-sm:px-4"><div className="mx-auto flex w-full max-w-6xl flex-col gap-5">
    <section className="flex flex-wrap items-end justify-between gap-4"><div><p className="m-0 text-xs font-semibold uppercase tracking-[0.12em] text-accent">{strings.financeCloseEyebrow}</p><h2 className="m-0 mt-1 text-xl font-semibold tracking-tight text-primary">{strings.financeCloseTitle}</h2><p className="m-0 mt-1 max-w-3xl text-sm text-secondary">{strings.financeCloseSubtitle}</p></div><Badge tone={lockDate ? "success" : "neutral"}><LockKeyhole className="size-3.5" />{lockDate ? strings.financeLockedThrough(lockDate) : strings.financeBooksOpen}</Badge></section>
    {error && <ErrorBanner message={error} />}
    {readiness && <Card pad="none" className="overflow-hidden"><div className="flex flex-wrap items-center justify-between gap-3 border-b border-subtle px-5 py-4"><div className="flex items-center gap-3"><span className={`grid size-10 place-items-center rounded-xl ${readiness.ready ? "bg-success-soft text-success" : "bg-[var(--accent-soft)] text-accent"}`}>{readiness.ready ? <CheckCircle2 className="size-5" /> : <CircleAlert className="size-5" />}</span><div><h3 className="m-0 text-base font-semibold text-primary">{readiness.ready ? strings.financeReadyToClose : strings.financeCloseNeedsAttention}</h3><p className="m-0 mt-0.5 text-sm text-secondary">{strings.financeCloseAsOf(readiness.on)}</p></div></div><div className="flex gap-2"><Badge tone={readiness.blockingCount ? "danger" : "success"}>{strings.financeBlockers(readiness.blockingCount)}</Badge>{readiness.warningCount > 0 && <Badge tone="warning">{strings.financeWarnings(readiness.warningCount)}</Badge>}</div></div><div className="grid divide-y divide-subtle lg:grid-cols-5 lg:divide-x lg:divide-y-0">{readiness.checks.map((check) => { const meta = CHECKS[check.key]; const Icon = check.status === "passed" ? CheckCircle2 : AlertTriangle; return <Link key={check.key} to={meta.href} className="flex min-w-0 items-center gap-3 px-4 py-4 text-primary no-underline transition-colors hover:bg-raised"><Icon className={`size-4 shrink-0 ${check.status === "passed" ? "text-success" : check.status === "warning" ? "text-warning" : "text-danger"}`} /><span className="min-w-0 flex-1"><span className="block truncate text-sm font-medium">{meta.label()}</span><span className="text-xs text-tertiary">{check.count === 0 ? strings.financeCheckPassed : strings.financeItems(check.count)}</span></span><ChevronRight className="size-4 text-tertiary" /></Link>; })}</div></Card>}
    <Card pad="none" className="overflow-hidden"><div className="border-b border-subtle px-5 py-4"><h3 className="m-0 text-base font-semibold text-primary">{strings.financePeriods}</h3><p className="m-0 mt-1 text-sm text-secondary">{strings.financePeriodsHint}</p></div><div className="grid gap-3 border-b border-subtle bg-raised px-5 py-4 md:grid-cols-[1fr_1fr_auto] md:items-end"><label className="text-sm font-medium text-primary">{strings.financeFrom}<Input className="mt-1.5" type="date" value={fromDate} onChange={(event) => setFromDate(event.target.value)} /></label><label className="text-sm font-medium text-primary">{strings.financeTo}<Input className="mt-1.5" type="date" value={toDate} onChange={(event) => setToDate(event.target.value)} /></label><Button disabled={!fromDate || !toDate || busy !== null} onClick={() => void create()}><Plus className="size-4" />{strings.financeAddPeriod}</Button></div><div className="divide-y divide-subtle">{periods.length === 0 ? <div className="px-5 py-10 text-center text-sm text-secondary">{strings.financeNoPeriods}</div> : [...periods].reverse().map((period) => <article key={period.id} className="flex flex-wrap items-center gap-4 px-5 py-4"><span className={`grid size-10 place-items-center rounded-xl ${period.status === "closed" ? "bg-success-soft text-success" : "bg-raised text-secondary"}`}>{period.status === "closed" ? <LockKeyhole className="size-4" /> : <LockOpen className="size-4" />}</span><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><span className="font-semibold text-primary">{period.fromDate} – {period.toDate}</span><Badge tone={period.status === "closed" ? "success" : "neutral"}>{period.status === "closed" ? strings.financeClosed : strings.financeOpen}</Badge></div>{period.closedAt && <p className="m-0 mt-1 text-xs text-secondary">{strings.financeClosedAt(new Date(period.closedAt).toLocaleString())}{period.note ? ` · ${period.note}` : ""}</p>}</div>{period.status === "open" ? <Button disabled={busy !== null || readiness?.on === period.toDate && !readiness.ready} onClick={() => void close(period)}>{strings.financeClosePeriod}</Button> : <Button variant="secondary" disabled={busy !== null} onClick={() => void reopen(period)}>{strings.financeReopenPeriod}</Button>}</article>)}</div></Card>
  </div></main>;
}
