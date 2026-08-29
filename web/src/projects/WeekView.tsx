// My week — the timesheet grid: rows are projects, columns are the seven days,
// cells are minutes.
//
// A plain table on purpose. A timesheet is a spreadsheet in everybody's head,
// and a nicer metaphor for it costs a person a week of not finding the Friday
// column (`docs/design/projects.md` § Web surface).
//
// Four rules the screen keeps:
//
// - **Every total is the server's.** The row totals, the day totals and the
//   week total are summed from the entries the server sent, which arrive with
//   the server's own `totals` beside them; the week total shown at the foot is
//   that field, not a number this file added up. A screen that computed its own
//   would be the second opinion an employee disputes.
// - **A locked week is read-only, and says why.** Submitted and approved weeks
//   refuse edits at the store; the grid disables the cells and the footer names
//   the status rather than offering a save that fails.
// - **A suggestion is not an hour.** Proposals arrive alongside real entries
//   saying which they are, and are counted only in `proposedMinutes` — so the
//   grid marks them and the totals do not silently include them. Deciding about
//   one is its own pair of verbs (B3.10b): accepting is what prices the hour and
//   puts it in the week, discarding removes something that was in no total, so
//   neither is offered as an edit and neither asks "are you sure" (the interface
//   laws' undo-over-confirm — a discarded suggestion costs a person nothing that
//   an agent cannot draft again).
// - **The week is addressed by its Monday**, because a week nobody has
//   submitted has no record yet and asking a person to create one first would
//   be a round trip that exists only to satisfy REST.
import { useEffect, useMemo, useState } from "react";
import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  Clock3,
  Plus,
  X,
} from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";

import { RecordAgentPanel } from "../agents";
import { Button, IconButton, Modal, Spinner, Table, Td, Th } from "../ds";
import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import { EntryDialog } from "./EntryDialog";
import {
  dayLabel,
  dayString,
  durationLabel,
  mondayOf,
  shiftWeek,
  weekDays,
} from "./format";
import { EmptyState, ErrorBanner, WeekChip } from "./parts";
import { ProjectScopePicker } from "./ProjectScopePicker";
import { resolveProjectScope } from "./scope";
import type { Project, TimeEntry, TimeTotals, TimesheetWeek } from "./types";
const styles = {
  page: "mx-auto flex min-h-0 w-full max-w-[112rem] flex-col gap-5 overflow-auto px-5 py-5 lg:px-7",
  toolbar: "flex flex-wrap items-center gap-3",
  periodLabel: "min-w-[15ch] text-base font-medium text-primary",
  toolbarSpacer: "flex-1",
  gridProject: "flex flex-col gap-0.5",
  gridProjectName: "font-medium",
  internal: "italic text-tertiary",
  gridCell:
    "w-full min-w-20 rounded-lg border border-transparent bg-transparent px-3 py-2 text-right text-sm tabular-nums text-primary transition-colors hover:border-default hover:bg-raised disabled:cursor-default disabled:text-tertiary",
  gridCellFilled: "font-medium",
  muted: "text-tertiary",
  rowActions: "flex items-center justify-end gap-2",
  weekFoot:
    "sticky bottom-0 z-10 flex flex-wrap items-center gap-4 rounded-2xl border border-default bg-surface/95 px-5 py-4 shadow-lg backdrop-blur",
  weekFootFacts: "flex flex-col gap-0.5",
  weekFootTotal: "text-lg font-semibold tabular-nums text-primary",
  weekFootNote: "m-0 text-sm text-tertiary",
  weekFootDecision: "m-0 max-w-[60ch] text-sm text-danger",
} as const;

/** Which cell of the grid is being written into. */
interface CellTarget {
  projectId: string;
  workDate: string;
  entry: TimeEntry | null;
}

/** The empty week a person who has never submitted one is looking at. The
 *  server has no row for it — `open` is the default the screen draws, not a
 *  record anybody invented. */
const OPEN_WEEK_STATUS = "open" as const;

/** The filled timesheet keeps its add action in the section header. An empty
 * week already has the same action in its empty state, so showing it in both
 * places creates two competing primary buttons for one operation. */
export function showTimesheetHeaderAddTime(
  rowCount: number,
  locked: boolean,
  projectCount: number,
): boolean {
  return rowCount > 0 && !locked && projectCount > 0;
}

export function WeekView({
  projects,
  projectsLoading,
  revision,
  onChanged,
}: {
  projects: Project[];
  projectsLoading: boolean;
  /** Bumped by the module when something outside this screen wrote an hour —
   *  the timer stopping, most of all. */
  revision: number;
  onChanged: () => void;
}) {
  const api = useProjectsApi();
  const [searchParams, setSearchParams] = useSearchParams();
  const requestedProjectId = searchParams.get("project");
  const projectId = resolveProjectScope(
    requestedProjectId,
    projectsLoading,
    projects,
  );
  const [monday, setMonday] = useState(() => mondayOf(new Date()));
  const [entries, setEntries] = useState<TimeEntry[]>([]);
  const [totals, setTotals] = useState<TimeTotals | null>(null);
  const [completeWeekTotals, setCompleteWeekTotals] =
    useState<TimeTotals | null>(null);
  const [week, setWeek] = useState<TimesheetWeek | null>(null);
  const [extraRows, setExtraRows] = useState<string[]>([]);
  const [target, setTarget] = useState<CellTarget | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  /** The suggestion a click is currently deciding about, so only its own two
   *  buttons go quiet — a whole grid disabled by one row's request reads as a
   *  broken screen. */
  const [deciding, setDeciding] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [choosingProject, setChoosingProject] = useState(false);

  const days = useMemo(() => weekDays(monday), [monday]);
  const sunday = days[6] ?? monday;
  const today = dayString(new Date());

  useEffect(() => {
    if (projectsLoading) return;
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const [period, completePeriod, weeks] = await Promise.all([
          api.time(monday, sunday, projectId ?? undefined),
          projectId === null ? Promise.resolve(null) : api.time(monday, sunday),
          api.weeks(monday, sunday),
        ]);
        if (!live) return;
        setEntries(period.entries);
        setTotals(period.totals);
        setCompleteWeekTotals(completePeriod?.totals ?? period.totals);
        setWeek(weeks.find((w) => w.weekStart === monday) ?? null);
        setError(null);
      } catch (err) {
        if (live) setError(projectsMessage(err, strings.projectsLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, monday, projectId, projectsLoading, sunday, reload, revision]);

  // Changing week starts a fresh set of rows: a project somebody added to look
  // at last week is not a project they are working on this one.
  useEffect(() => {
    setExtraRows([]);
  }, [monday]);

  const status = week?.status ?? OPEN_WEEK_STATUS;
  const locked = week?.locked ?? false;

  /** The rows: every project worked this week, in the engagement list's order,
   *  plus the ones this person has opened a row for. */
  const rows = useMemo(() => {
    const worked = new Set(entries.map((e) => e.projectId));
    return projects.filter(
      (p) =>
        (projectId === null || p.id === projectId) &&
        (worked.has(p.id) || extraRows.includes(p.id)),
    );
  }, [projects, entries, extraRows, projectId]);

  /** The entries in one cell. A cell is a project on a day, and a person can
   *  have written more than one — two sittings on the same job is not a
   *  mistake, and merging them would erase the notes. */
  function cell(projectId: string, workDate: string): TimeEntry[] {
    return entries.filter(
      (e) => e.projectId === projectId && e.workDate === workDate,
    );
  }

  /** The board's own name, or its id when the row belongs to a project this
   *  list no longer carries (an archived one somebody worked on). */
  function projectName(projectId: string): string {
    return projects.find((p) => p.id === projectId)?.name ?? projectId;
  }

  /** Minutes in a set of entries, proposals excluded — a suggestion invisibly
   *  inside a total is not a suggestion. */
  function minutesOf(set: TimeEntry[]): number {
    return set.reduce((sum, e) => (e.proposed ? sum : sum + e.minutes), 0);
  }

  function openCell(projectId: string, workDate: string) {
    if (locked) return;
    // Exactly one real entry: the click means "correct that". More than one is
    // ambiguous, so it means "add another" and the list below is where each of
    // them is corrected. A **proposal** is never what a click lands on: it is
    // not an hour until a human accepts it, and accepting is its own verb
    // (ADR 0023, B3.10) rather than a correction of something already written.
    const written = cell(projectId, workDate).filter((e) => !e.proposed);
    setTarget({
      projectId,
      workDate,
      entry: written.length === 1 ? (written[0] ?? null) : null,
    });
  }

  /** Accept or discard one suggestion. The row is reloaded rather than patched
   *  in place: accepting is what resolves the rate and moves the minutes into
   *  the week's totals, and both of those are the server's answer, not this
   *  screen's arithmetic. */
  async function decideEntry(entry: TimeEntry, verdict: "accept" | "reject") {
    setDeciding(entry.id);
    setError(null);
    try {
      if (verdict === "accept") await api.acceptTime(entry.id);
      else await api.rejectTime(entry.id);
      setReload((r) => r + 1);
      onChanged();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
    } finally {
      setDeciding(null);
    }
  }

  async function decide(action: "submit" | "withdraw") {
    setBusy(true);
    setError(null);
    try {
      const next =
        action === "submit"
          ? await api.submitWeek(monday)
          : await api.withdrawWeek(monday);
      setWeek(next);
      onChanged();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  function startEntry(projectId: string) {
    const workDate = days.includes(today) ? today : monday;
    setExtraRows((current) =>
      current.includes(projectId) ? current : [...current, projectId],
    );
    setChoosingProject(false);
    setTarget({ projectId, workDate, entry: null });
  }

  return (
    <div className={styles.page}>
      <section className="rounded-2xl border border-default bg-surface shadow-sm">
        <div className="flex flex-wrap items-start justify-between gap-4 px-5 py-4">
          <div className="flex min-w-0 items-start gap-3">
            <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
              <CalendarDays size={20} aria-hidden="true" />
            </span>
            <div className="min-w-0">
              <h2 className="m-0 text-lg font-semibold text-primary">
                {strings.projectsWeekTitle}
              </h2>
              <p className="m-0 mt-0.5 text-sm text-secondary">
                {strings.projectsWeekPurpose}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <WeekChip status={status} />
            {loading && <Spinner size={16} />}
            {showTimesheetHeaderAddTime(
              rows.length,
              locked,
              projects.length,
            ) && (
              <Button
                icon={<Plus size={17} />}
                onClick={() => {
                  if (projectId !== null) startEntry(projectId);
                  else setChoosingProject(true);
                }}
              >
                {strings.projectsAddTime}
              </Button>
            )}
          </div>
        </div>
        <div className="grid gap-4 border-t border-subtle px-5 py-4 lg:grid-cols-[minmax(16rem,22rem)_1fr] lg:items-end">
          <ProjectScopePicker
            compact
            projects={projects}
            value={projectId}
            disabled={projectsLoading}
            onChange={(nextProjectId) => {
              const next = new URLSearchParams(searchParams);
              if (nextProjectId === null) next.delete("project");
              else next.set("project", nextProjectId);
              setSearchParams(next);
            }}
          />
          <div className="flex flex-wrap items-center gap-1 lg:justify-end">
            <Button
              variant="ghost"
              size="sm"
              icon={<ChevronLeft size={16} />}
              aria-label={strings.projectsPreviousWeek}
              onClick={() => setMonday(shiftWeek(monday, -1))}
            />
            <span className="min-w-[15ch] px-2 text-center text-sm font-semibold text-primary">
              {strings.projectsWeekOf(
                dayLabel(monday, { day: "numeric", month: "short" }),
                dayLabel(sunday),
              )}
            </span>
            <Button
              variant="ghost"
              size="sm"
              icon={<ChevronRight size={16} />}
              aria-label={strings.projectsNextWeek}
              onClick={() => setMonday(shiftWeek(monday, 1))}
            />
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setMonday(mondayOf(new Date()))}
            >
              {strings.projectsThisWeek}
            </Button>
          </div>
        </div>
      </section>

      {error !== null && <ErrorBanner message={error} />}

      {rows.length === 0 && !loading ? (
        <EmptyState
          Icon={CalendarDays}
          title={strings.projectsWeekEmptyTitle}
          body={strings.projectsWeekEmptyBody}
          {...(!locked && projects.length > 0
            ? {
                cta: strings.projectsAddTime,
                onCta: () => {
                  if (projectId !== null) startEntry(projectId);
                  else setChoosingProject(true);
                },
              }
            : {})}
        />
      ) : (
        <section className="overflow-hidden rounded-2xl border border-default bg-surface shadow-sm">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-subtle px-5 py-4">
            <div>
              <h3 className="m-0 text-base font-semibold text-primary">
                {strings.projectsWeek}
              </h3>
              <p className="m-0 mt-0.5 text-sm text-secondary">
                {strings.projectsWeekOf(dayLabel(monday), dayLabel(sunday))}
              </p>
            </div>
            <div className="flex items-center gap-2 rounded-xl bg-raised px-3 py-2">
              <Clock3 size={16} className="text-accent" aria-hidden="true" />
              <span className="text-sm font-semibold tabular-nums text-primary">
                {durationLabel(totals?.minutes ?? 0)}
              </span>
            </div>
          </div>
          <Table label={strings.projectsTabWeek} flat>
              <thead>
                <tr>
                  <Th className="min-w-64">{strings.projectsProject}</Th>
                  {days.map((day) => (
                    <Th key={day} numeric>
                      {/* Today's column heading keeps its accent: the span's
                          own colour wins over the table's header ink. */}
                      {day === today ? (
                        <span className="text-accent">
                          {dayLabel(day, { weekday: "short", day: "numeric" })}
                        </span>
                      ) : (
                        dayLabel(day, { weekday: "short", day: "numeric" })
                      )}
                    </Th>
                  ))}
                  <Th numeric>{strings.projectsTotal}</Th>
                </tr>
              </thead>
              <tbody>
                {rows.map((project) => {
                  const rowMinutes = minutesOf(
                    entries.filter((e) => e.projectId === project.id),
                  );
                  return (
                    <tr key={project.id}>
                      <td>
                        <span className={styles.gridProject}>
                          <span className={styles.gridProjectName}>
                            {project.name}
                          </span>
                          {project.client === null && (
                            <span className={styles.internal}>
                              {strings.projectsInternal}
                            </span>
                          )}
                        </span>
                      </td>
                      {days.map((day) => {
                        const set = cell(project.id, day);
                        const minutes = minutesOf(set);
                        const proposed = set.some((e) => e.proposed);
                        return (
                          <td key={day}>
                            <button
                              type="button"
                              className={`${styles.gridCell} ${minutes > 0 ? styles.gridCellFilled : ""}`}
                              disabled={locked}
                              aria-label={strings.projectsCellLabel(
                                project.name,
                                dayLabel(day),
                                durationLabel(minutes),
                              )}
                              onClick={() => openCell(project.id, day)}
                            >
                              {minutes === 0 && !proposed
                                ? ""
                                : durationLabel(minutes)}
                              {set.length > 1 && (
                                <span className={styles.muted}>
                                  {" "}
                                  ·{set.length}
                                </span>
                              )}
                              {proposed && (
                                <span className={styles.muted}> ✦</span>
                              )}
                            </button>
                          </td>
                        );
                      })}
                      <Td numeric className="font-semibold">
                        {durationLabel(rowMinutes)}
                      </Td>
                    </tr>
                  );
                })}
              </tbody>
              <tfoot>
                <tr>
                  <Td className="font-semibold">{strings.projectsTotal}</Td>
                  {days.map((day) => (
                    <Td key={day} numeric className="font-semibold">
                      {durationLabel(
                        minutesOf(entries.filter((e) => e.workDate === day)),
                      )}
                    </Td>
                  ))}
                  {/* The week's own figure, as the server counted it. */}
                  <Td numeric className="font-semibold">
                    {durationLabel(totals?.minutes ?? 0)}
                  </Td>
                </tr>
              </tfoot>
          </Table>
        </section>
      )}

      {choosingProject && !locked && (
        <Modal
          title={strings.projectsChooseTimeProject}
          onClose={() => setChoosingProject(false)}
          wide
          actions={
            <IconButton
              label={strings.close}
              icon={<X size={18} />}
              onClick={() => setChoosingProject(false)}
            />
          }
        >
          <p className="m-0 text-sm text-secondary">
            {strings.projectsChooseTimeProjectHint}
          </p>
          <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
            {projects.map((project) => (
              <button
                key={project.id}
                type="button"
                className="flex min-h-14 items-center justify-between gap-3 rounded-xl border border-default bg-surface px-4 py-3 text-left transition-colors hover:border-accent hover:bg-accent-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                onClick={() => startEntry(project.id)}
              >
                <span className="min-w-0">
                  <span className="block truncate font-medium text-primary">
                    {project.name}
                  </span>
                  <span className="mt-0.5 block truncate text-xs text-secondary">
                    {project.client === null
                      ? strings.projectsInternal
                      : strings.projectsCustomer}
                  </span>
                </span>
                <ChevronRight
                  size={17}
                  className="shrink-0 text-secondary"
                  aria-hidden="true"
                />
              </button>
            ))}
          </div>
        </Modal>
      )}

      {/* Every entry of the week, one row each. The grid answers "how much, on
          what, when"; this answers "which of them, and what did it say" — and
          it is where the second entry in a cell is reached, since a cell with
          two sittings in it cannot mean both at once. */}
      {entries.length > 0 && (
        <Table label={strings.projectsWeekEntriesLabel} interactiveRows>
            <thead>
              <tr>
                <Th>{strings.projectsDay}</Th>
                <Th>{strings.projectsProject}</Th>
                <Th>{strings.projectsTask}</Th>
                <Th>{strings.projectsNote}</Th>
                <Th numeric>{strings.projectsDuration}</Th>
                <Th hideLabel>{strings.projectsActions}</Th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.id}>
                  {/* The table's own cell ink is the primary colour; the span's
                      wins, which is how a muted cell stays muted. */}
                  <td>
                    <span className={styles.muted}>
                      {dayLabel(entry.workDate, {
                        weekday: "short",
                        day: "numeric",
                        month: "short",
                      })}
                    </span>
                  </td>
                  <td>{projectName(entry.projectId)}</td>
                  <td>
                    {entry.taskId !== null && entry.taskTitle ? (
                      <Link
                        className="inline-flex max-w-64 items-center rounded-md px-2 py-1 font-medium text-primary !no-underline transition-colors hover:bg-raised hover:text-accent hover:!no-underline focus:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                        to={`/projects/${encodeURIComponent(entry.projectId)}/list?open=${encodeURIComponent(entry.taskId)}`}
                      >
                        <span className="truncate">{entry.taskTitle}</span>
                      </Link>
                    ) : (
                      <span className={styles.muted}>—</span>
                    )}
                  </td>
                  <td>
                    <span className={styles.muted}>
                      {entry.note === "" ? strings.projectsNoNote : entry.note}
                      {!entry.billable && ` · ${strings.projectsNotBillable}`}
                      {entry.proposed && ` · ${strings.projectsProposedEntry}`}
                      {entry.billed && ` · ${strings.projectsBilledEntry}`}
                    </span>
                  </td>
                  <Td numeric>{durationLabel(entry.minutes)}</Td>
                  <td>
                    <div className={styles.rowActions}>
                      {entry.proposed ? (
                        // A suggestion is accepted or discarded, never
                        // corrected: correcting one would file an hour the
                        // person never agreed happened.
                        <>
                          <Button
                            size="sm"
                            disabled={locked || deciding === entry.id}
                            aria-label={strings.projectsAcceptEntryLabel(
                              projectName(entry.projectId),
                              durationLabel(entry.minutes),
                            )}
                            onClick={() => void decideEntry(entry, "accept")}
                          >
                            {strings.projectsAcceptEntry}
                          </Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            disabled={locked || deciding === entry.id}
                            aria-label={strings.projectsRejectEntryLabel(
                              projectName(entry.projectId),
                              durationLabel(entry.minutes),
                            )}
                            onClick={() => void decideEntry(entry, "reject")}
                          >
                            {strings.projectsRejectEntry}
                          </Button>
                        </>
                      ) : (
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={locked}
                          onClick={() =>
                            setTarget({
                              projectId: entry.projectId,
                              workDate: entry.workDate,
                              entry,
                            })
                          }
                        >
                          {strings.projectsEdit}
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
        </Table>
      )}

      {/* The week is the timesheet's record in focus. A never-submitted week
          has no stored row yet, so its Monday — the address every timesheet
          read uses — stands in as the id. */}
      <RecordAgentPanel
        product="projects"
        recordKind="timesheet"
        recordId={week?.id ?? monday}
        recordLabel={strings.projectsWeekOf(dayLabel(monday), dayLabel(sunday))}
        origin={null}
      />

      <div className={styles.weekFoot}>
        <div className={styles.weekFootFacts}>
          <span className="text-xs font-semibold uppercase tracking-wide text-tertiary">
            {projectId === null
              ? strings.projectsCompleteWeek
              : strings.projectsCompleteWeekSubmission}
          </span>
          <span className={styles.weekFootTotal}>
            {durationLabel(completeWeekTotals?.minutes ?? 0)}
          </span>
          <p className={styles.weekFootNote}>
            {strings.projectsBillableOfWeek(
              durationLabel(completeWeekTotals?.billableMinutes ?? 0),
            )}
          </p>
          {(totals?.proposedMinutes ?? 0) > 0 && (
            <>
              <p className={styles.weekFootNote}>
                {strings.projectsProposedInWeek(
                  durationLabel(totals?.proposedMinutes ?? 0),
                )}
              </p>
              {/* What to do about them, where they are: the list above is the
                  only place a suggestion is decided, and a person who has just
                  asked an agent to draft a month needs telling once. */}
              <p className={styles.weekFootNote}>
                {strings.projectsSuggestionsWaiting(
                  entries.filter((e) => e.proposed).length,
                )}
              </p>
            </>
          )}
        </div>
        <span className={styles.toolbarSpacer} />
        {status === "rejected" && week !== null && week.decisionNote !== "" && (
          <p className={styles.weekFootDecision}>
            {strings.projectsRejectedBecause(week.decisionNote)}
          </p>
        )}
        {status === "submitted" ? (
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => void decide("withdraw")}
          >
            {strings.projectsWithdrawWeek}
          </Button>
        ) : (
          <Button
            disabled={
              busy ||
              status === "approved" ||
              (completeWeekTotals?.minutes ?? 0) === 0
            }
            onClick={() => void decide("submit")}
          >
            {strings.projectsSubmitWeek}
          </Button>
        )}
      </div>

      {target !== null && (
        <EntryDialog
          entry={target.entry}
          projectId={target.projectId}
          workDate={target.workDate}
          projects={projects}
          onClose={() => setTarget(null)}
          onSaved={() => {
            setTarget(null);
            setReload((r) => r + 1);
            onChanged();
          }}
        />
      )}
    </div>
  );
}
