// The receipt for an agent action the user already approved (ADR 0034). Most
// tools produce a record and nothing more to say — those get the one-line
// confirmation the overlay always showed. The two Projects tools (B3.10a)
// produce something worth reading back: a suggested timesheet entry, and a
// project's figures.
//
// The figures come from the server as numbers only — it composes no sentence,
// because a sentence written in the server is a user-facing string in one
// language (CLAUDE.md). Every word around them is written here, in the
// catalogue, so the summary is in the reader's language.
import { CalendarClock, Gauge } from "lucide-react";

import { strings } from "../i18n";
import type {
  AgentResultDto,
  ProjectStatusResultDto,
  TimeEntryResultDto,
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
  // Every other tool: the confirmation this overlay has always shown.
  return <p className={styles.note}>{strings.agentDone}</p>;
}
