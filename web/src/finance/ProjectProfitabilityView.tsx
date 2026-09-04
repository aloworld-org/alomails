import { useCallback, useEffect, useState } from "react";
import type { ReactNode } from "react";
import {
  ArrowRight,
  BriefcaseBusiness,
  CircleAlert,
  Gauge,
  ReceiptText,
} from "lucide-react";
import { Link } from "react-router-dom";

import {
  formatAmount,
  quarterOf,
  type Period,
} from "../billing";
import { ReportPeriodPicker } from "../crm/ReportPeriodPicker";
import { Card, Spinner } from "../ds";
import { getLocale, strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "../projects/api";
import type {
  ProfitabilityCurrency,
  ProfitabilityReport,
  ProjectProfitability,
} from "../projects/types";
import { dayLabel } from "./format";
import { ErrorBanner } from "./parts";

export function ProjectProfitabilityView() {
  const api = useProjectsApi();
  const [period, setPeriod] = useState<Period>(() => quarterOf(new Date()));
  const [report, setReport] = useState<ProfitabilityReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setReport(await api.profitability(period.from, period.to));
      setError(null);
    } catch (reason) {
      setError(projectsMessage(reason, strings.financeProfitabilityLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, period]);

  useEffect(() => {
    void load();
  }, [load]);

  const exceptions =
    report?.projects.filter(
      (project) =>
        (project.budgetConsumptionBp ?? 0) > 10_000 ||
        project.unratedMinutes > 0 ||
        project.byCurrency.some((row) => row.unbilledNetCents > 0),
    ).length ?? 0;

  return (
    <main className="min-h-0 flex-1 overflow-auto px-8 py-6 max-sm:px-4">
      <div className="mx-auto flex w-full max-w-[108rem] flex-col gap-5">
        <section className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <p className="m-0 text-xs font-semibold uppercase tracking-[0.12em] text-accent">
              {strings.financeProfitabilityEyebrow}
            </p>
            <h2 className="m-0 mt-1 text-xl font-semibold tracking-tight text-primary">
              {strings.financeProfitabilityTitle}
            </h2>
            <p className="m-0 mt-1 text-sm text-secondary">
              {strings.financeProfitabilitySubtitle}
            </p>
          </div>
          <Link
            to="/projects/reports"
            className="inline-flex items-center gap-1.5 text-sm font-semibold text-accent hover:underline"
          >
            {strings.financeOpenProjects}
            <ArrowRight className="size-4" />
          </Link>
        </section>
        <Card pad="none" className="overflow-visible">
          <div className="flex min-h-20 flex-wrap items-center gap-3 p-4">
            <ReportPeriodPicker value={period} onApply={setPeriod} />
            {loading && <span className="ml-auto inline-flex"><Spinner size={16} /></span>}
          </div>
          {report !== null && (
            <section className="grid border-t border-subtle md:grid-cols-3 md:[&>*+*]:border-l md:[&>*+*]:border-t-0 [&>*+*]:border-t [&>*+*]:border-subtle">
              <Summary
                Icon={BriefcaseBusiness}
                label={strings.financeActiveEngagements}
                value={String(report.projects.length)}
              />
              <Summary
                Icon={CircleAlert}
                label={strings.financeProfitabilityExceptions}
                value={String(exceptions)}
                danger={exceptions > 0}
              />
              <Summary
                Icon={ReceiptText}
                label={strings.financeUnbilledValue}
                value={
                  <Money
                    rows={report.totals.byCurrency}
                    field="unbilledNetCents"
                  />
                }
              />
            </section>
          )}
        </Card>
        {error !== null && <ErrorBanner message={error} />}
        {report !== null && (
          <>
            {report.projects.length === 0 ? (
              <Card pad="lg" className="text-center">
                <BriefcaseBusiness className="mx-auto size-8 text-tertiary" />
                <h3 className="m-0 mt-3 text-base font-semibold text-primary">
                  {strings.financeProfitabilityEmpty}
                </h3>
                <p className="m-0 mt-1 text-sm text-secondary">
                  {strings.financeProfitabilityEmptyHint}
                </p>
              </Card>
            ) : (
              <section className="grid gap-3 xl:grid-cols-2">
                {report.projects.map((project) => (
                  <ProjectCard key={project.projectId} project={project} />
                ))}
              </section>
            )}
            <p className="m-0 text-xs text-tertiary">
              {strings.financeProfitabilityBasis(
                dayLabel(report.from),
                dayLabel(report.to),
              )}
            </p>
          </>
        )}
      </div>
    </main>
  );
}

function ProjectCard({ project }: { project: ProjectProfitability }) {
  const consumption = project.budgetConsumptionBp;
  const over = consumption !== null && consumption > 10_000;
  const width =
    consumption === null ? 0 : Math.min(100, Math.round(consumption / 100));
  return (
    <Card as="article" pad="md">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="m-0 truncate text-base font-semibold text-primary">
            {project.projectName}
          </h3>
          <p className="m-0 mt-1 text-xs text-secondary">
            {strings.financeProjectPeriodValue}
          </p>
        </div>
        {over && (
          <span className="rounded-full bg-danger-soft px-2.5 py-1 text-xs font-semibold text-danger">
            {strings.financeOverBudget}
          </span>
        )}
      </div>
      <div className="mt-4 grid grid-cols-2 gap-4">
        <div>
          <p className="m-0 text-xs text-secondary">
            {strings.financeEarnedValue}
          </p>
          <div className="mt-1 font-semibold text-primary">
            <Money rows={project.byCurrency} field="netCents" />
          </div>
        </div>
        <div>
          <p className="m-0 text-xs text-secondary">
            {strings.financeUnbilledValue}
          </p>
          <div className="mt-1 font-semibold text-primary">
            <Money rows={project.byCurrency} field="unbilledNetCents" />
          </div>
        </div>
      </div>
      {consumption !== null && (
        <div className="mt-4">
          <div className="mb-1.5 flex justify-between text-xs">
            <span className="text-secondary">{strings.financeBudgetUsed}</span>
            <span
              className={
                over
                  ? "font-semibold text-danger"
                  : "font-semibold text-primary"
              }
            >
              {Math.round(consumption / 100)}%
            </span>
          </div>
          <div className="h-2 overflow-hidden rounded-full bg-raised">
            <div
              className={`h-full rounded-full ${over ? "bg-danger" : "bg-accent"}`}
              style={{ width: `${width}%` }}
            />
          </div>
          {project.budgetRemainingCents !== null && (
            <p className="m-0 mt-2 text-xs text-secondary">
              {strings.financeBudgetRemaining(
                formatAmount(
                  project.budgetRemainingCents,
                  getLocale(),
                  project.currency,
                ),
              )}
            </p>
          )}
        </div>
      )}
      <div className="mt-4 flex flex-wrap gap-2">
        {project.unratedMinutes > 0 && (
          <span className="inline-flex items-center gap-1 rounded-full bg-warning-soft px-2.5 py-1 text-xs font-medium text-warning">
            <CircleAlert className="size-3.5" />
            {strings.financeUnratedMinutes(project.unratedMinutes)}
          </span>
        )}
        {project.budgetCents === null && (
          <span className="inline-flex items-center gap-1 rounded-full bg-raised px-2.5 py-1 text-xs font-medium text-secondary">
            <Gauge className="size-3.5" />
            {strings.financeNoMoneyBudget}
          </span>
        )}
      </div>
    </Card>
  );
}

function Money({
  rows,
  field,
}: {
  rows: ProfitabilityCurrency[];
  field: "netCents" | "unbilledNetCents";
}) {
  return (
    <>
      {rows.length === 0
        ? "—"
        : rows.map((row) => (
            <span className="block" key={row.currency}>
              {formatAmount(row[field], getLocale(), row.currency)}
            </span>
          ))}
    </>
  );
}
function Summary({
  Icon,
  label,
  value,
  danger = false,
}: {
  Icon: typeof BriefcaseBusiness;
  label: string;
  value: ReactNode;
  danger?: boolean;
}) {
  return (
    <div className="flex min-w-0 items-center gap-3 p-4">
        <span
          className={`grid size-10 place-items-center rounded-xl ${danger ? "bg-danger-soft text-danger" : "bg-[var(--accent-soft)] text-accent"}`}
        >
          <Icon className="size-5" />
        </span>
        <div>
          <p className="m-0 text-xs text-secondary">{label}</p>
          <div
            className={`mt-1 text-lg font-semibold ${danger ? "text-danger" : "text-primary"}`}
          >
            {value}
          </div>
        </div>
    </div>
  );
}
