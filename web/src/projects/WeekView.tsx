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
import { CalendarDays, ChevronLeft, ChevronRight, Plus } from "lucide-react";

import { Button, Spinner } from "../ds";
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
import type { Project, TimeEntry, TimeTotals, TimesheetWeek } from "./types";
import styles from "./ProjectsModule.module.css";

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

export function WeekView({
  projects,
  revision,
  onChanged,
}: {
  projects: Project[];
  /** Bumped by the module when something outside this screen wrote an hour —
   *  the timer stopping, most of all. */
  revision: number;
  onChanged: () => void;
}) {
  const api = useProjectsApi();
  const [monday, setMonday] = useState(() => mondayOf(new Date()));
  const [entries, setEntries] = useState<TimeEntry[]>([]);
  const [totals, setTotals] = useState<TimeTotals | null>(null);
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

  const days = useMemo(() => weekDays(monday), [monday]);
  const sunday = days[6] ?? monday;
  const today = dayString(new Date());

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const [period, weeks] = await Promise.all([
          api.time(monday, sunday),
          api.weeks(monday, sunday),
        ]);
        if (!live) return;
        setEntries(period.entries);
        setTotals(period.totals);
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
  }, [api, monday, sunday, reload, revision]);

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
    return projects.filter((p) => worked.has(p.id) || extraRows.includes(p.id));
  }, [projects, entries, extraRows]);

  const unusedProjects = projects.filter((p) => !rows.some((row) => row.id === p.id));

  /** The entries in one cell. A cell is a project on a day, and a person can
   *  have written more than one — two sittings on the same job is not a
   *  mistake, and merging them would erase the notes. */
  function cell(projectId: string, workDate: string): TimeEntry[] {
    return entries.filter((e) => e.projectId === projectId && e.workDate === workDate);
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
        action === "submit" ? await api.submitWeek(monday) : await api.withdrawWeek(monday);
      setWeek(next);
      onChanged();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={styles.page}>
      <div className={styles.toolbar}>
        <Button
          variant="ghost"
          size="sm"
          icon={<ChevronLeft size={16} />}
          onClick={() => setMonday(shiftWeek(monday, -1))}
        >
          {strings.projectsPreviousWeek}
        </Button>
        <span className={styles.periodLabel}>
          {strings.projectsWeekOf(dayLabel(monday, { day: "numeric", month: "short" }), dayLabel(sunday))}
        </span>
        <Button
          variant="ghost"
          size="sm"
          icon={<ChevronRight size={16} />}
          onClick={() => setMonday(shiftWeek(monday, 1))}
        >
          {strings.projectsNextWeek}
        </Button>
        <Button variant="ghost" size="sm" onClick={() => setMonday(mondayOf(new Date()))}>
          {strings.projectsThisWeek}
        </Button>
        <span className={styles.toolbarSpacer} />
        <WeekChip status={status} />
        {loading && <Spinner size={16} />}
      </div>

      {error !== null && <ErrorBanner message={error} />}

      {rows.length === 0 && !loading ? (
        <EmptyState
          Icon={CalendarDays}
          title={strings.projectsWeekEmptyTitle}
          body={strings.projectsWeekEmptyBody}
        />
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.grid}>
            <thead>
              <tr>
                <th scope="col">{strings.projectsProject}</th>
                {days.map((day) => (
                  <th
                    key={day}
                    scope="col"
                    className={day === today ? styles.gridToday : undefined}
                  >
                    {dayLabel(day, { weekday: "short", day: "numeric" })}
                  </th>
                ))}
                <th scope="col">{strings.projectsTotal}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((project) => {
                const rowMinutes = minutesOf(entries.filter((e) => e.projectId === project.id));
                return (
                  <tr key={project.id}>
                    <td>
                      <span className={styles.gridProject}>
                        <span className={styles.gridProjectName}>{project.name}</span>
                        {project.client === null && (
                          <span className={styles.internal}>{strings.projectsInternal}</span>
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
                            {minutes === 0 && !proposed ? "" : durationLabel(minutes)}
                            {set.length > 1 && (
                              <span className={styles.muted}> ·{set.length}</span>
                            )}
                            {proposed && <span className={styles.muted}> ✦</span>}
                          </button>
                        </td>
                      );
                    })}
                    <td className={styles.numeric}>{durationLabel(rowMinutes)}</td>
                  </tr>
                );
              })}
            </tbody>
            <tfoot>
              <tr className={styles.gridTotals}>
                <td>{strings.projectsTotal}</td>
                {days.map((day) => (
                  <td key={day}>
                    {durationLabel(minutesOf(entries.filter((e) => e.workDate === day)))}
                  </td>
                ))}
                {/* The week's own figure, as the server counted it. */}
                <td>{durationLabel(totals?.minutes ?? 0)}</td>
              </tr>
            </tfoot>
          </table>
        </div>
      )}

      {!locked && unusedProjects.length > 0 && (
        <div className={styles.toolbar}>
          <Plus size={16} aria-hidden="true" />
          <select
            className={styles.select}
            value=""
            aria-label={strings.projectsAddRow}
            onChange={(e) => {
              if (e.target.value !== "") setExtraRows((rows) => [...rows, e.target.value]);
            }}
          >
            <option value="">{strings.projectsAddRow}</option>
            {unusedProjects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Every entry of the week, one row each. The grid answers "how much, on
          what, when"; this answers "which of them, and what did it say" — and
          it is where the second entry in a cell is reached, since a cell with
          two sittings in it cannot mean both at once. */}
      {entries.length > 0 && (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.projectsDay}</th>
                <th scope="col">{strings.projectsProject}</th>
                <th scope="col">{strings.projectsNote}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.projectsDuration}
                </th>
                <th scope="col" aria-label={strings.projectsActions} />
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.id}>
                  <td className={styles.muted}>
                    {dayLabel(entry.workDate, { weekday: "short", day: "numeric", month: "short" })}
                  </td>
                  <td>{projectName(entry.projectId)}</td>
                  <td className={styles.muted}>
                    {entry.note === "" ? strings.projectsNoNote : entry.note}
                    {!entry.billable && ` · ${strings.projectsNotBillable}`}
                    {entry.proposed && ` · ${strings.projectsProposedEntry}`}
                    {entry.billed && ` · ${strings.projectsBilledEntry}`}
                  </td>
                  <td className={styles.numeric}>{durationLabel(entry.minutes)}</td>
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
          </table>
        </div>
      )}

      <div className={styles.weekFoot}>
        <div className={styles.weekFootFacts}>
          <span className={styles.weekFootTotal}>{durationLabel(totals?.minutes ?? 0)}</span>
          <p className={styles.weekFootNote}>
            {strings.projectsBillableOfWeek(durationLabel(totals?.billableMinutes ?? 0))}
          </p>
          {(totals?.proposedMinutes ?? 0) > 0 && (
            <>
              <p className={styles.weekFootNote}>
                {strings.projectsProposedInWeek(durationLabel(totals?.proposedMinutes ?? 0))}
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
          <Button variant="ghost" disabled={busy} onClick={() => void decide("withdraw")}>
            {strings.projectsWithdrawWeek}
          </Button>
        ) : (
          <Button
            disabled={busy || status === "approved" || (totals?.minutes ?? 0) === 0}
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
