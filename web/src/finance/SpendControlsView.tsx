import { useEffect, useState } from "react";
import { CheckCircle2, FileCheck2, FolderKanban, ShieldCheck, UsersRound } from "lucide-react";

import { hundredthsToInput, parseHundredths } from "../billing";
import { Button, Card, Checkbox, Input, Spinner } from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { ErrorBanner } from "./parts";
import type { SpendPolicy } from "./types";

type RuleKey = "receiptRequiredAboveCents" | "projectRequiredAboveCents" | "secondApprovalAboveCents";
const EMPTY: SpendPolicy = { receiptRequiredAboveCents: null, projectRequiredAboveCents: null, secondApprovalAboveCents: null, currency: "EUR", updatedBy: null, updatedAt: null };

export function SpendControlsView() {
  const api = useFinanceApi();
  const [policy, setPolicy] = useState(EMPTY);
  const [values, setValues] = useState<Record<RuleKey, string>>({ receiptRequiredAboveCents: "", projectRequiredAboveCents: "", secondApprovalAboveCents: "" });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => { let live = true; void api.spendPolicy().then((answer) => { if (!live) return; setPolicy(answer); setValues({ receiptRequiredAboveCents: answer.receiptRequiredAboveCents === null ? "" : hundredthsToInput(answer.receiptRequiredAboveCents), projectRequiredAboveCents: answer.projectRequiredAboveCents === null ? "" : hundredthsToInput(answer.projectRequiredAboveCents), secondApprovalAboveCents: answer.secondApprovalAboveCents === null ? "" : hundredthsToInput(answer.secondApprovalAboveCents) }); }).catch((reason: unknown) => { if (live) setError(financeMessage(reason, strings.financePolicyLoadFailed)); }).finally(() => { if (live) setLoading(false); }); return () => { live = false; }; }, [api]);

  function toggle(key: RuleKey, enabled: boolean) { setValues((current) => ({ ...current, [key]: enabled ? (current[key] || "0.00") : "" })); setSaved(false); }
  function amount(key: RuleKey): number | null | undefined { if (values[key] === "") return null; return parseHundredths(values[key]) ?? undefined; }
  const valid = (Object.keys(values) as RuleKey[]).every((key) => amount(key) !== undefined);

  async function save() { if (!valid) return; setSaving(true); setError(null); try { const answer = await api.saveSpendPolicy({ receiptRequiredAboveCents: amount("receiptRequiredAboveCents") ?? null, projectRequiredAboveCents: amount("projectRequiredAboveCents") ?? null, secondApprovalAboveCents: amount("secondApprovalAboveCents") ?? null }); setPolicy(answer); setSaved(true); } catch (reason) { setError(financeMessage(reason, strings.financePolicySaveFailed)); } finally { setSaving(false); } }

  if (loading) return <div className="grid flex-1 place-items-center"><Spinner size={22} /></div>;
  return <main className="min-h-0 flex-1 overflow-auto px-8 py-6 max-sm:px-4"><div className="mx-auto flex w-full max-w-5xl flex-col gap-5">
    <section><p className="m-0 text-xs font-semibold uppercase tracking-[0.12em] text-accent">{strings.financePolicyEyebrow}</p><h2 className="m-0 mt-1 text-xl font-semibold tracking-tight text-primary">{strings.financePolicyTitle}</h2><p className="m-0 mt-1 max-w-3xl text-sm text-secondary">{strings.financePolicySubtitle}</p></section>
    {error !== null && <ErrorBanner message={error} />}
    <Card pad="none" className="overflow-hidden"><div className="flex items-start gap-3 border-b border-subtle px-5 py-4"><span className="grid size-10 place-items-center rounded-xl bg-[var(--accent-soft)] text-accent"><ShieldCheck className="size-5" /></span><div><h3 className="m-0 text-base font-semibold text-primary">{strings.financePolicyRules}</h3><p className="m-0 mt-1 text-sm text-secondary">{strings.financePolicyRulesHint(policy.currency)}</p></div></div><div className="divide-y divide-subtle">
      <Rule Icon={FileCheck2} title={strings.financeReceiptRule} body={strings.financeReceiptRuleHint} enabled={values.receiptRequiredAboveCents !== ""} value={values.receiptRequiredAboveCents} currency={policy.currency} invalid={amount("receiptRequiredAboveCents") === undefined} onToggle={(enabled) => toggle("receiptRequiredAboveCents", enabled)} onChange={(value) => { setValues((current) => ({ ...current, receiptRequiredAboveCents: value })); setSaved(false); }} />
      <Rule Icon={FolderKanban} title={strings.financeProjectRule} body={strings.financeProjectRuleHint} enabled={values.projectRequiredAboveCents !== ""} value={values.projectRequiredAboveCents} currency={policy.currency} invalid={amount("projectRequiredAboveCents") === undefined} onToggle={(enabled) => toggle("projectRequiredAboveCents", enabled)} onChange={(value) => { setValues((current) => ({ ...current, projectRequiredAboveCents: value })); setSaved(false); }} />
      <Rule Icon={UsersRound} title={strings.financeSecondApprovalRule} body={strings.financeSecondApprovalRuleHint} enabled={values.secondApprovalAboveCents !== ""} value={values.secondApprovalAboveCents} currency={policy.currency} invalid={amount("secondApprovalAboveCents") === undefined} onToggle={(enabled) => toggle("secondApprovalAboveCents", enabled)} onChange={(value) => { setValues((current) => ({ ...current, secondApprovalAboveCents: value })); setSaved(false); }} />
    </div><div className="flex items-center justify-end gap-3 border-t border-subtle bg-raised px-5 py-4">{saved && <span className="inline-flex items-center gap-1.5 text-sm font-medium text-success"><CheckCircle2 className="size-4" />{strings.financePolicySaved}</span>}<Button disabled={!valid || saving} onClick={() => void save()}>{saving ? strings.financeSaving : strings.financeSavePolicy}</Button></div></Card>
  </div></main>;
}

function Rule({ Icon, title, body, enabled, value, currency, invalid, onToggle, onChange }: { Icon: typeof FileCheck2; title: string; body: string; enabled: boolean; value: string; currency: string; invalid: boolean; onToggle: (enabled: boolean) => void; onChange: (value: string) => void }) { return <div className="grid gap-4 px-5 py-5 lg:grid-cols-[minmax(0,1fr)_18rem] lg:items-center"><div className="flex items-start gap-3"><span className="grid size-9 shrink-0 place-items-center rounded-xl bg-raised text-secondary"><Icon className="size-4" /></span><div><h4 className="m-0 text-sm font-semibold text-primary">{title}</h4><p className="m-0 mt-1 max-w-2xl text-sm text-secondary">{body}</p></div></div><div className="flex items-center gap-3"><Checkbox checked={enabled} onChange={onToggle} label={enabled ? strings.financePolicyEnabled : strings.financePolicyDisabled} /><div className="relative min-w-0 flex-1"><Input value={value} disabled={!enabled} inputMode="decimal" aria-label={title} onChange={(event) => onChange(event.target.value)} className={invalid ? "border-danger pr-14" : "pr-14"} /><span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs font-semibold text-tertiary">{currency}</span></div></div></div>; }
