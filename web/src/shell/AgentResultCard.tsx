// The receipt for an agent action the user already approved (ADR 0034). Most
// tools produce a record and nothing more to say — those get the one-line
// confirmation the overlay always showed. The three Projects tools produce
// something worth reading back: a suggested timesheet entry, a project's
// figures (B3.10a), and a batch drafted from the diary (B3.10b).
//
// The figures come from the server as numbers only — it composes no sentence,
// because a sentence written in the server is a user-facing string in one
// language (CLAUDE.md). Every word around them is written here, in the
// catalogue, so the summary is in the reader's language.
import { CalendarClock, CalendarRange, Gauge } from "lucide-react";

import { strings } from "../i18n";
import type {
  AgentResultDto,
  ProjectStatusResultDto,
  TimeEntryResultDto,
  TimesheetDraftResultDto,
} from "../jmap";
import { dayLabel, durationLabel, percentLabel } from "../projects/format";
import styles from "./AgentResultCard.module.css";

/** A `projectStatus` result, checked at runtime: the `kind` alone is a string
 *  the server could widen, so the shape is what is trusted. */
function isProjectStatus(result: AgentResultDto): result is ProjectStatusResultDto {
  return result.kind === "projectStatus" && "hours" in result && "milestones" in result;
}

/** A `timeEntry` result — always a proposal on this path (`log_time` writes no
 *  other kind), and the card says so rather than implying the hour is counted. */
function isTimeEntry(result: AgentResultDto): result is TimeEntryResultDto {
  return result.kind === "timeEntry" && "minutes" in result && "workDate" in result;
}

/** A `timesheetDraft` result — a batch drafted from the caller's Agenda. Both
 *  lists may be empty (a diary with nothing in it), so the shape is what is
 *  checked, not their contents. */
function isTimesheetDraft(result: AgentResultDto): result is TimesheetDraftResultDto {
  return result.kind === "timesheetDraft" && "drafted" in result && "skipped" in result;
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
        {aside !== undefined && aside !== "" && <span className={styles.aside}> · {aside}</span>}
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
        <Row label={strings.agentFieldDuration} value={durationLabel(entry.minutes)} />
        {entry.note !== "" && <Row label={strings.projectsNote} value={entry.note} />}
      </div>
      <p className={styles.note}>{strings.agentTimeLogged(entry.title ?? "")}</p>
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
          value={strings.agentDraftedRange(dayLabel(draft.from), dayLabel(draft.to))}
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
                  <span className={styles.aside}> · {strings.agentDraftedOverlap}</span>
                )}
              </span>
              <span className={styles.itemMinutes}>{durationLabel(entry.minutes)}</span>
            </li>
          ))}
        </ul>
      )}
      {draft.skipped.length > 0 && (
        <>
          <span className={styles.groupLabel}>{strings.agentDraftedLeftOut}</span>
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
        <p className={styles.note}>{strings.agentDraftedOverlaps(draft.overlaps)}</p>
      )}
      {draft.drafted.length > 0 && (
        <p className={styles.note}>{strings.agentDraftedNote(draft.title ?? "")}</p>
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
          value={hours.minutes === 0 ? strings.agentStatusNeverWorked : durationLabel(hours.minutes)}
          aside={
            hours.billableMinutes > 0
              ? strings.agentStatusBillable(durationLabel(hours.billableMinutes))
              : undefined
          }
        />
        {hours.lastWorkedOn !== null && (
          <Row label={strings.agentStatusLastWorked} value={dayLabel(hours.lastWorkedOn)} />
        )}
        {budget.isClientWork ? (
          <>
            {budget.customer !== undefined && budget.customer !== null && (
              <Row label={strings.agentStatusCustomer} value={budget.customer} />
            )}
            <Row
              label={strings.agentStatusBudget}
              value={
                budget.budgetMinutes === undefined || budget.budgetMinutes === null
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
          <Row label={strings.agentStatusBudget} value={strings.agentStatusInternal} />
        )}
        <Row
          label={strings.agentStatusMilestones}
          value={
            milestones.total === 0
              ? strings.agentStatusNoMilestones
              : strings.agentStatusMilestonesDone(milestones.done, milestones.total)
          }
          aside={
            milestones.late > 0 ? strings.agentStatusMilestonesLate(milestones.late) : undefined
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
          aside={tasks.overdue > 0 ? strings.agentStatusTasksOverdue(tasks.overdue) : undefined}
        />
      </div>
      <p className={styles.note}>{strings.agentProjectStatusNote}</p>
    </div>
  );
}

export function AgentResultCard({ result }: { result: AgentResultDto }) {
  if (isProjectStatus(result)) return <ProjectStatusResult status={result} />;
  if (isTimeEntry(result)) return <TimeEntryResult entry={result} />;
  if (isTimesheetDraft(result)) return <TimesheetDraftResult draft={result} />;
  // Every other tool: the confirmation this overlay has always shown.
  return <p className={styles.note}>{strings.agentDone}</p>;
}
