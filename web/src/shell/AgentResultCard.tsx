// The receipt for an agent action the user already approved (ADR 0034). Most
// tools produce a record and nothing more to say — those get the one-line
// confirmation the overlay always showed. The three Projects tools produce
// something worth reading back: a suggested timesheet entry, a project's
// figures (B3.10a), and a batch drafted from the diary (B3.10b). The finance
// agent's suggested categories (B4.14a) go further and are *answerable* here —
// each one is accepted or declined on the spot, because a suggestion a person
// cannot answer where they see it is one they never answer at all.
//
// The figures come from the server as numbers only — it composes no sentence,
// because a sentence written in the server is a user-facing string in one
// language (CLAUDE.md). Every word around them is written here, in the
// catalogue, so the summary is in the reader's language.
import { useState } from "react";
import {
  CalendarClock,
  CalendarRange,
  Gauge,
  PackageSearch,
  Percent,
  ScanSearch,
  ShoppingCart,
  Tags,
} from "lucide-react";

import { formatAmount, formatRate } from "../billing";
import { Button } from "../ds";
import { financeMessage, useFinanceApi } from "../finance";
import { getLocale, strings } from "../i18n";
import type {
  AgentResultDto,
  AnomalyFindingDto,
  CategoryProposalsResultDto,
  JournalAnomaliesResultDto,
  ProjectStatusResultDto,
  ReorderProposalsResultDto,
  StockAnswerResultDto,
  TimeEntryResultDto,
  TimesheetDraftResultDto,
  VatSummaryResultDto,
  VatSummarySideDto,
} from "../jmap";
import { qtyLabel } from "../inventory/format";
import { dayLabel, durationLabel, percentLabel } from "../projects/format";
import styles from "./AgentResultCard.module.css";

/** A `projectStatus` result, checked at runtime: the `kind` alone is a string
 *  the server could widen, so the shape is what is trusted. */
function isProjectStatus(
  result: AgentResultDto,
): result is ProjectStatusResultDto {
  return (
    result.kind === "projectStatus" &&
    "hours" in result &&
    "milestones" in result
  );
}

/** A `timeEntry` result — always a proposal on this path (`log_time` writes no
 *  other kind), and the card says so rather than implying the hour is counted. */
function isTimeEntry(result: AgentResultDto): result is TimeEntryResultDto {
  return (
    result.kind === "timeEntry" && "minutes" in result && "workDate" in result
  );
}

/** A `timesheetDraft` result — a batch drafted from the caller's Agenda. Both
 *  lists may be empty (a diary with nothing in it), so the shape is what is
 *  checked, not their contents. */
function isTimesheetDraft(
  result: AgentResultDto,
): result is TimesheetDraftResultDto {
  return (
    result.kind === "timesheetDraft" &&
    "drafted" in result &&
    "skipped" in result
  );
}

/** A `categoryProposals` result — the finance agent's suggestions (B4.14a).
 *  Both lists may be empty (a period with nothing unclassified in it), so the
 *  shape is what is checked, not their contents. */
function isCategoryProposals(
  result: AgentResultDto,
): result is CategoryProposalsResultDto {
  return (
    result.kind === "categoryProposals" &&
    "proposed" in result &&
    "skipped" in result
  );
}

/** A `vatSummary` result — the VAT figures the books carry (B4.14b). Both sides
 *  are always present, however empty the period was, so the shape is what is
 *  checked. */
function isVatSummary(result: AgentResultDto): result is VatSummaryResultDto {
  return (
    result.kind === "vatSummary" &&
    "output" in result &&
    "netPayableCents" in result
  );
}

/** A `journalAnomalies` result — what a scan of the journal found (B4.14b). An
 *  empty `findings` is a real and useful answer ("nothing stood out"), so the
 *  shape is what is checked, not its contents. */
function isJournalAnomalies(
  result: AgentResultDto,
): result is JournalAnomaliesResultDto {
  return (
    result.kind === "journalAnomalies" &&
    "findings" in result &&
    "scanned" in result
  );
}

/** A `reorderProposals` result — the drafts the inventory agent wrote (B5.10).
 *  Both lists may be empty (a warehouse with nothing under its minimum), so the
 *  shape is what is checked, not their contents. */
function isReorderProposals(
  result: AgentResultDto,
): result is ReorderProposalsResultDto {
  return (
    result.kind === "reorderProposals" &&
    "drafted" in result &&
    "skipped" in result
  );
}

/** A `stockAnswer` result — where one product stands (B5.10). A product with
 *  nothing anywhere is a real answer, so the shape is what is checked. */
function isStockAnswer(result: AgentResultDto): result is StockAnswerResultDto {
  return (
    result.kind === "stockAnswer" &&
    "stock" in result &&
    "availableQtyMilli" in result
  );
}

/** One labelled figure. `aside` is a second fact on the same line ("3 open ·
 *  1 past due"); an absent or empty one is simply not drawn, so a caller can
 *  pass the string it has without deciding first. */
function Row({
  label,
  value,
  aside,
}: {
  label: string;
  value: string;
  aside?: string | undefined;
}) {
  return (
    <div className={styles.row}>
      <span className={styles.label}>{label}</span>
      <span className={styles.value}>
        {value}
        {aside !== undefined && aside !== "" && (
          <span className={styles.aside}> · {aside}</span>
        )}
      </span>
    </div>
  );
}

/** The suggested entry, as the timesheet will show it. */
function TimeEntryResult({ entry }: { entry: TimeEntryResultDto }) {
  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <CalendarClock size={16} aria-hidden />
        <span>{strings.agentActLogTime}</span>
      </div>
      <div className={styles.rows}>
        <Row label={strings.agentFieldProject} value={entry.title ?? ""} />
        <Row label={strings.agentFieldDay} value={dayLabel(entry.workDate)} />
        <Row
          label={strings.agentFieldDuration}
          value={durationLabel(entry.minutes)}
        />
        {entry.note !== "" && (
          <Row label={strings.projectsNote} value={entry.note} />
        )}
      </div>
      <p className={styles.note}>
        {strings.agentTimeLogged(entry.title ?? "")}
      </p>
    </div>
  );
}

/** The batch: what was drafted, what was left out, and why. Every entry in it
 *  is a suggestion — the card says so once, at the end, rather than on each of
 *  twenty lines.
 *
 *  The reasons arrive as codes and are turned into words here: a server that
 *  wrote the sentence would write it in one language. A code this build does not
 *  know still reads as "left out", never as nothing. */
function TimesheetDraftResult({ draft }: { draft: TimesheetDraftResultDto }) {
  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <CalendarRange size={16} aria-hidden />
        <span>{strings.agentActDraftTimesheet}</span>
      </div>
      <div className={styles.rows}>
        <Row label={strings.agentFieldProject} value={draft.title ?? ""} />
        <Row
          label={strings.agentFieldDay}
          value={strings.agentDraftedRange(
            dayLabel(draft.from),
            dayLabel(draft.to),
          )}
        />
        <Row
          label={strings.agentDraftedTotal}
          value={
            draft.drafted.length === 0
              ? strings.agentDraftedNone
              : durationLabel(draft.minutes)
          }
          aside={
            draft.drafted.length === 0
              ? undefined
              : strings.agentDraftedCount(draft.drafted.length)
          }
        />
      </div>
      {draft.drafted.length > 0 && (
        <ul className={styles.list}>
          {draft.drafted.map((entry) => (
            <li key={entry.id} className={styles.item}>
              <span className={styles.itemDay}>{dayLabel(entry.workDate)}</span>
              <span className={styles.itemName}>
                {entry.note}
                {entry.overlaps && (
                  <span className={styles.aside}>
                    {" "}
                    · {strings.agentDraftedOverlap}
                  </span>
                )}
              </span>
              <span className={styles.itemMinutes}>
                {durationLabel(entry.minutes)}
              </span>
            </li>
          ))}
        </ul>
      )}
      {draft.skipped.length > 0 && (
        <>
          <span className={styles.groupLabel}>
            {strings.agentDraftedLeftOut}
          </span>
          <ul className={styles.list}>
            {draft.skipped.map((skipped, i) => (
              <li
                // Nothing in a skipped line is an id — a meeting left out has no
                // record — so its position is the only stable key it has.
                key={`${skipped.day}-${i}`}
                className={`${styles.item} ${styles.itemSkipped}`}
              >
                <span className={styles.itemDay}>{dayLabel(skipped.day)}</span>
                <span className={styles.itemName}>
                  {skipped.summary}
                  <span className={styles.aside}>
                    {" "}
                    · {strings.agentDraftedReason(skipped.reason)}
                  </span>
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
      {draft.overlaps > 0 && (
        <p className={styles.note}>
          {strings.agentDraftedOverlaps(draft.overlaps)}
        </p>
      )}
      {draft.drafted.length > 0 && (
        <p className={styles.note}>
          {strings.agentDraftedNote(draft.title ?? "")}
        </p>
      )}
    </div>
  );
}

/** Where the project stands: hours, budget, plan, work. Every block that has
 *  nothing to report says so in words — an absent budget is "no budget set",
 *  never a zero, which would read as a budget of nothing. */
function ProjectStatusResult({ status }: { status: ProjectStatusResultDto }) {
  const { hours, budget, milestones, tasks } = status;
  const consumption = budget.consumptionBp;
  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <Gauge size={16} aria-hidden />
        <span>{status.title ?? strings.agentActProjectStatus}</span>
      </div>
      <div className={styles.rows}>
        <Row
          label={strings.agentStatusHours}
          value={
            hours.minutes === 0
              ? strings.agentStatusNeverWorked
              : durationLabel(hours.minutes)
          }
          aside={
            hours.billableMinutes > 0
              ? strings.agentStatusBillable(
                  durationLabel(hours.billableMinutes),
                )
              : undefined
          }
        />
        {hours.lastWorkedOn !== null && (
          <Row
            label={strings.agentStatusLastWorked}
            value={dayLabel(hours.lastWorkedOn)}
          />
        )}
        {budget.isClientWork ? (
          <>
            {budget.customer !== undefined && budget.customer !== null && (
              <Row
                label={strings.agentStatusCustomer}
                value={budget.customer}
              />
            )}
            <Row
              label={strings.agentStatusBudget}
              value={
                budget.budgetMinutes === undefined ||
                budget.budgetMinutes === null
                  ? strings.agentStatusNoBudget
                  : durationLabel(budget.budgetMinutes)
              }
              aside={
                consumption === undefined || consumption === null
                  ? undefined
                  : strings.agentStatusBudgetUsed(percentLabel(consumption))
              }
            />
          </>
        ) : (
          <Row
            label={strings.agentStatusBudget}
            value={strings.agentStatusInternal}
          />
        )}
        <Row
          label={strings.agentStatusMilestones}
          value={
            milestones.total === 0
              ? strings.agentStatusNoMilestones
              : strings.agentStatusMilestonesDone(
                  milestones.done,
                  milestones.total,
                )
          }
          aside={
            milestones.late > 0
              ? strings.agentStatusMilestonesLate(milestones.late)
              : undefined
          }
        />
        {milestones.next !== null && (
          <Row
            label={strings.agentStatusNext}
            value={milestones.next.name}
            aside={dayLabel(milestones.next.dueOn)}
          />
        )}
        <Row
          label={strings.agentStatusTasks}
          value={strings.agentStatusTasksOpen(tasks.open)}
          aside={
            tasks.overdue > 0
              ? strings.agentStatusTasksOverdue(tasks.overdue)
              : undefined
          }
        />
      </div>
      <p className={styles.note}>{strings.agentProjectStatusNote}</p>
    </div>
  );
}

/** The suggestions, each with the two verbs that answer it.
 *
 *  The verbs are here rather than only on the Finance screen because this is
 *  where the person is looking when the suggestions are made, and a suggestion
 *  nobody can answer where they see it is one they never answer at all. Each
 *  answer is one call about one claim: the batch was suggested together, and it
 *  is agreed to one at a time (ADR 0023).
 *
 *  An answered line keeps its place and says what was answered — removing it
 *  would move every line below it under the reader's cursor. */
function CategoryProposalsResult({
  proposals,
}: {
  proposals: CategoryProposalsResultDto;
}) {
  const api = useFinanceApi();
  const [answered, setAnswered] = useState<
    Record<string, "accepted" | "declined">
  >({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function answer(id: string, verb: "accepted" | "declined") {
    setBusy(id);
    setError(null);
    try {
      if (verb === "accepted") await api.acceptExpenseCategory(id);
      else await api.declineExpenseCategory(id);
      setAnswered((done) => ({ ...done, [id]: verb }));
    } catch (err) {
      // The server's own sentence, which names the rule it refused on — a claim
      // handed in while the card was open is the case a person must be told
      // about rather than left clicking.
      setError(financeMessage(err, strings.agentCategoriseFailed));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <Tags size={16} aria-hidden />
        <span>{strings.agentActCategorise}</span>
      </div>
      <div className={styles.rows}>
        <Row
          label={strings.agentCategoriseFieldPeriod}
          value={strings.agentDraftedRange(
            dayLabel(proposals.from),
            dayLabel(proposals.to),
          )}
        />
        <Row
          label={strings.agentDraftedTotal}
          value={
            proposals.proposed.length === 0
              ? strings.agentCategoriseNone
              : strings.agentCategoriseSuggested(proposals.proposed.length)
          }
          aside={strings.agentCategoriseConsidered(proposals.considered)}
        />
      </div>
      {proposals.proposed.length > 0 && (
        <ul className={styles.list}>
          {proposals.proposed.map((one) => {
            const done = answered[one.id];
            return (
              <li key={one.id} className={styles.answerable}>
                <span className={styles.itemDay}>
                  {one.spentOn === null ? "" : dayLabel(one.spentOn)}
                </span>
                <span className={styles.answerName}>
                  {one.merchant === null || one.merchant === ""
                    ? strings.agentCategoriseNoMerchant
                    : one.merchant}
                  <span className={styles.aside}>
                    {" "}
                    · {strings.agentCategoriseEvidence(one.evidence)}
                  </span>
                </span>
                <span className={styles.answerCategory}>
                  {one.categoryName ?? one.categoryId}
                </span>
                {done === undefined ? (
                  <span className={styles.answerVerbs}>
                    <Button
                      size="sm"
                      disabled={busy !== null}
                      onClick={() => void answer(one.id, "accepted")}
                    >
                      {strings.agentCategoriseAccept}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={busy !== null}
                      onClick={() => void answer(one.id, "declined")}
                    >
                      {strings.agentCategoriseDecline}
                    </Button>
                  </span>
                ) : (
                  <span className={styles.answered}>
                    {done === "accepted"
                      ? strings.agentCategoriseAccepted
                      : strings.agentCategoriseDeclined}
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {proposals.skipped.length > 0 && (
        <>
          <span className={styles.groupLabel}>
            {strings.agentCategoriseLeftOut}
          </span>
          <ul className={styles.list}>
            {proposals.skipped.map((skipped) => (
              <li
                key={skipped.id}
                className={`${styles.item} ${styles.itemSkipped}`}
              >
                <span className={styles.itemDay}>
                  {skipped.spentOn === null ? "" : dayLabel(skipped.spentOn)}
                </span>
                <span className={styles.itemName}>
                  {skipped.merchant === null || skipped.merchant === ""
                    ? strings.agentCategoriseNoMerchant
                    : skipped.merchant}
                  <span className={styles.aside}>
                    {" "}
                    · {strings.agentCategoriseReason(skipped.reason)}
                  </span>
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
      {error !== null && <p className={styles.note}>{error}</p>}
      {proposals.proposed.length > 0 && (
        <p className={styles.note}>{strings.agentCategoriseFooter}</p>
      )}
    </div>
  );
}

/** One side of the VAT figures: the rates the period actually used, what was on
 *  no rate, and the total. Every amount is the server's, in the accounting
 *  currency it stated — the browser adds nothing up. */
function VatSide({
  label,
  base,
  side,
  currency,
}: {
  label: string;
  base: string;
  side: VatSummarySideDto;
  currency: string;
}) {
  const money = (cents: number) => formatAmount(cents, getLocale(), currency);
  return (
    <>
      <span className={styles.groupLabel}>{label}</span>
      <div className={styles.rows}>
        {side.rates.map((rate) => (
          <Row
            key={rate.rateBp}
            label={strings.agentVatRateRow(
              formatRate(rate.rateBp, getLocale()),
              money(rate.baseCents),
            )}
            value={money(rate.vatCents)}
          />
        ))}
        {(side.unratedBaseCents !== 0 || side.unratedVatCents !== 0) && (
          <Row
            label={strings.agentVatUnrated}
            value={money(side.unratedVatCents)}
            aside={money(side.unratedBaseCents)}
          />
        )}
        <Row
          label={base}
          value={money(side.vatCents)}
          aside={money(side.baseCents)}
        />
      </div>
    </>
  );
}

/** The VAT figures, read back. Nothing was filed, and the card says so at the
 *  end rather than leaving a person to assume it. */
function VatSummaryResult({ report }: { report: VatSummaryResultDto }) {
  const money = (cents: number) =>
    formatAmount(cents, getLocale(), report.currency);
  const owed = report.netPayableCents >= 0;
  const empty =
    report.output.rates.length === 0 &&
    report.input.rates.length === 0 &&
    report.netPayableCents === 0;
  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <Percent size={16} aria-hidden />
        <span>{strings.agentActVatSummary}</span>
      </div>
      <div className={styles.rows}>
        <Row
          label={strings.agentVatFieldPeriod}
          value={strings.agentDraftedRange(
            dayLabel(report.from),
            dayLabel(report.to),
          )}
        />
      </div>
      {empty ? (
        <p className={styles.note}>{strings.agentVatNothing}</p>
      ) : (
        <>
          <VatSide
            label={strings.agentVatCharged}
            base={strings.agentVatBaseSales}
            side={report.output}
            currency={report.currency}
          />
          <VatSide
            label={strings.agentVatPaid}
            base={strings.agentVatBaseCosts}
            side={report.input}
            currency={report.currency}
          />
          <div className={styles.rows}>
            {/* The sign is the server's; which way round it reads is written
                here, because "you owe" and "you are owed" are words. */}
            <Row
              label={owed ? strings.agentVatOwed : strings.agentVatRefund}
              value={money(Math.abs(report.netPayableCents))}
            />
          </div>
        </>
      )}
      <p className={styles.note}>{strings.agentVatFooter}</p>
    </div>
  );
}

/** One finding: what kind of question it is, the account and counterparty it is
 *  about, and — always — the entries behind it. */
function AnomalyFinding({
  finding,
  currency,
}: {
  finding: AnomalyFindingDto;
  currency: string;
}) {
  const money = (cents: number) => formatAmount(cents, getLocale(), currency);
  const where = [
    finding.accountName ?? finding.accountCode,
    finding.counterparty?.name,
  ]
    .filter((part) => part !== null && part !== undefined && part !== "")
    .join(" · ");
  return (
    <li className={styles.item}>
      <span className={styles.itemName}>
        {strings.agentAnomalyKind(finding.kind)}
        {where !== "" && <span className={styles.aside}> · {where}</span>}
      </span>
      <span className={styles.itemMinutes}>
        {finding.missingMonth === null
          ? money(finding.amountCents)
          : strings.agentAnomalyMissingMonth(
              dayLabel(finding.missingMonth, {
                month: "long",
                year: "numeric",
              }),
            )}
        {finding.typicalCents !== null && (
          <span className={styles.aside}>
            {" "}
            · {strings.agentAnomalyTypical(money(finding.typicalCents))}
          </span>
        )}
      </span>
      {/* The argument for the finding. A flag without the rows that caused it is
          an accusation, so this list is never collapsed away. */}
      <ul className={styles.list}>
        {finding.entries.map((entry) => (
          <li key={entry.id} className={`${styles.item} ${styles.itemSkipped}`}>
            <span className={styles.itemDay}>{dayLabel(entry.entryDate)}</span>
            <span className={styles.itemName}>{entry.memo}</span>
            <span className={styles.itemMinutes}>
              {money(entry.amountCents)}
            </span>
          </li>
        ))}
      </ul>
    </li>
  );
}

/** What a scan of the journal found — and, just as much part of the answer,
 *  what it could not look at. */
function JournalAnomaliesResult({ scan }: { scan: JournalAnomaliesResultDto }) {
  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <ScanSearch size={16} aria-hidden />
        <span>{strings.agentActFlagAnomalies}</span>
      </div>
      <div className={styles.rows}>
        <Row
          label={strings.agentAnomalyFieldPeriod}
          value={strings.agentDraftedRange(
            dayLabel(scan.from),
            dayLabel(scan.to),
          )}
        />
        <Row
          label={
            scan.found === 0
              ? strings.agentAnomalyNone
              : strings.agentAnomalyFound(scan.found)
          }
          value={strings.agentAnomalyScanned(scan.scanned)}
          aside={
            scan.shown < scan.found
              ? strings.agentAnomalyShown(scan.shown, scan.found)
              : undefined
          }
        />
      </div>
      {scan.findings.length > 0 && (
        <ul className={styles.list}>
          {scan.findings.map((finding, i) => (
            <AnomalyFinding
              // A finding is not a record and has no id of its own — it is a
              // question about a period — so its place in the answer is the only
              // stable key it has.
              key={`${finding.kind}-${finding.accountId}-${i}`}
              finding={finding}
              currency={scan.currency}
            />
          ))}
        </ul>
      )}
      {scan.truncated && (
        <p className={styles.note}>{strings.agentAnomalyTruncated}</p>
      )}
      {scan.notComparable > 0 && (
        <p className={styles.note}>
          {strings.agentAnomalyNotComparable(scan.notComparable)}
        </p>
      )}
      <p className={styles.note}>{strings.agentAnomalyFooter}</p>
    </div>
  );
}

/** The drafts, and — just as much part of the answer — what was left out.
 *
 *  Every amount and every quantity here is the server's. The card's whole job
 *  is to keep a draft a draft: the footer says no supplier has been contacted,
 *  and it is the last thing on the card because it is the one sentence a reader
 *  must not miss. */
function ReorderProposalsResult({
  proposals,
}: {
  proposals: ReorderProposalsResultDto;
}) {
  const money = (cents: number, currency: string) =>
    formatAmount(cents, getLocale(), currency);
  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <ShoppingCart size={16} aria-hidden />
        <span>{strings.agentActReorderProposals}</span>
      </div>
      <div className={styles.rows}>
        <Row
          label={strings.agentFieldSupplier}
          value={
            proposals.supplier?.supplierName ??
            strings.agentReorderEverySupplier
          }
        />
        <Row
          label={strings.agentFieldLocation}
          value={
            proposals.location === null
              ? strings.agentReorderEverywhere
              : `${proposals.location.locationCode} · ${proposals.location.locationName}`
          }
        />
        <Row
          label={
            proposals.shortages === 0
              ? strings.agentReorderNothingShort
              : strings.agentReorderShortages(proposals.shortages)
          }
          value={strings.agentReorderDrafted(proposals.drafted.length)}
        />
      </div>
      {proposals.drafted.length > 0 && (
        <ul className={styles.list}>
          {proposals.drafted.map((draft) => (
            <li key={draft.id} className={styles.item}>
              <span className={styles.itemName}>
                {draft.supplierName}
                <span className={styles.aside}>
                  {" "}
                  · {strings.agentReorderLines(draft.lineCount)}
                </span>
              </span>
              <span className={styles.itemMinutes}>
                {money(draft.totals.grossCents, draft.currency)}
              </span>
            </li>
          ))}
        </ul>
      )}
      {proposals.skipped.length > 0 && (
        <>
          <span className={styles.groupLabel}>
            {strings.agentReorderLeftOut}
          </span>
          <ul className={styles.list}>
            {proposals.skipped.map((skipped) => (
              <li
                key={`${skipped.productId}-${skipped.locationCode}`}
                className={`${styles.item} ${styles.itemSkipped}`}
              >
                <span className={styles.itemName}>
                  {skipped.productName}
                  <span className={styles.aside}>
                    {" "}
                    · {strings.agentReorderReason(skipped.reason)}
                  </span>
                </span>
                <span className={styles.itemMinutes}>
                  {qtyLabel(skipped.buyQtyMilli)}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
      {proposals.drafted.length > 0 && (
        <p className={styles.note}>{strings.agentReorderFooter}</p>
      )}
    </div>
  );
}

/** Where one product stands. The four quantities are shown together because
 *  each explains the next: what is on the shelf, what is coming, what is
 *  promised away, and what that leaves. */
function StockAnswerResult({ answer }: { answer: StockAnswerResultDto }) {
  const unit = answer.product.unit;
  const qty = (milli: number) =>
    unit === "" ? qtyLabel(milli) : `${qtyLabel(milli)} ${unit}`;
  if (!answer.product.stocked) {
    return (
      <div className={styles.card}>
        <div className={styles.header}>
          <PackageSearch size={16} aria-hidden />
          <span>{answer.title ?? strings.agentActStockAnswer}</span>
        </div>
        <p className={styles.note}>{strings.agentStockNoShelf}</p>
      </div>
    );
  }
  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <PackageSearch size={16} aria-hidden />
        <span>{answer.title ?? strings.agentActStockAnswer}</span>
      </div>
      <div className={styles.rows}>
        <Row
          label={strings.agentStockOnHand}
          value={
            answer.stock.length === 0
              ? strings.agentStockNowhere
              : qty(answer.onHandQtyMilli)
          }
        />
        {answer.onOrderQtyMilli !== 0 && (
          <Row
            label={strings.agentStockOnOrder}
            value={qty(answer.onOrderQtyMilli)}
          />
        )}
        {answer.committedQtyMilli !== 0 && (
          <Row
            label={strings.agentStockCommitted}
            value={qty(answer.committedQtyMilli)}
          />
        )}
        <Row
          label={strings.agentStockAvailable}
          value={qty(answer.availableQtyMilli)}
        />
      </div>
      {answer.stock.length > 0 && (
        <ul className={styles.list}>
          {answer.stock.map((level) => (
            <li key={level.locationId} className={styles.item}>
              <span className={styles.itemDay}>{level.locationCode}</span>
              <span className={styles.itemName}>{level.locationName}</span>
              <span className={styles.itemMinutes}>{qty(level.qtyMilli)}</span>
            </li>
          ))}
        </ul>
      )}
      {answer.watched.length > 0 && (
        <>
          <span className={styles.groupLabel}>{strings.agentStockWatched}</span>
          <ul className={styles.list}>
            {answer.watched.map((watch) => (
              <li
                key={watch.locationId}
                className={`${styles.item}${watch.belowMinimum ? "" : ` ${styles.itemSkipped}`}`}
              >
                <span className={styles.itemDay}>{watch.locationCode}</span>
                <span className={styles.itemName}>
                  {strings.agentStockMinimum(
                    qtyLabel(watch.minQtyMilli),
                    qtyLabel(watch.targetQtyMilli),
                  )}
                  {watch.belowMinimum && (
                    <span className={styles.aside}>
                      {" "}
                      · {strings.agentStockBelowMinimum}
                    </span>
                  )}
                </span>
                <span className={styles.itemMinutes}>
                  {qty(watch.onHandQtyMilli)}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
      <p className={styles.note}>{strings.agentStockFooter}</p>
    </div>
  );
}

export function AgentResultCard({ result }: { result: AgentResultDto }) {
  if (isProjectStatus(result)) return <ProjectStatusResult status={result} />;
  if (isTimeEntry(result)) return <TimeEntryResult entry={result} />;
  if (isTimesheetDraft(result)) return <TimesheetDraftResult draft={result} />;
  if (isCategoryProposals(result))
    return <CategoryProposalsResult proposals={result} />;
  if (isVatSummary(result)) return <VatSummaryResult report={result} />;
  if (isJournalAnomalies(result))
    return <JournalAnomaliesResult scan={result} />;
  if (isReorderProposals(result))
    return <ReorderProposalsResult proposals={result} />;
  if (isStockAnswer(result)) return <StockAnswerResult answer={result} />;
  // Every other tool: the confirmation this overlay has always shown.
  return <p className={styles.note}>{strings.agentDone}</p>;
}
