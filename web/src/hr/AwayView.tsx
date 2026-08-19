// Who's away (alo HR, ADR 0035, wave B6.08b): the month, and the people not in
// it.
//
// This is the module's one screen about other people, and it is every member's
// — the same reasoning `hr_leave_balances.rs` gives for the route behind it. A
// balance is a figure *about a person* and goes to them, their manager and HR;
// who is not here on Thursday is a fact *about a team*, and a workspace that
// makes you ask a person for it is a workspace with a filing cabinet in it.
//
// What it shows is exactly what the route serves: a name, an employee id and a
// day. There is no policy here, no kind of leave and no note — not because this
// screen strips them, but because the store's query does not select them, so
// there is nothing here to forget to hide. "Away" is the whole of what a
// colleague learns.
//
// The tenant's public holidays are drawn behind the same grid (B6.04), because
// a week with a holiday in it is why somebody's five days off cost four, and
// because a company's calendar is the other half of "can I take that week".
import { useEffect, useMemo, useState } from "react";
import { CalendarDays, ChevronLeft, ChevronRight } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import { Button, Spinner, Toolbar } from "../ds";
import { strings } from "../i18n";
import { hrMessage, useHrApi } from "./api";
import {
  absenceIndex,
  browserToday,
  holidayIndex,
  isWeekend,
  monthLabel,
  monthOf,
  monthWeeks,
  peopleAway,
  shiftMonth,
  weekdayNames,
  yearsOf,
} from "./leave";
import { EmptyState, ErrorBanner } from "./parts";
import type { HrAbsenceDay } from "./types";
import styles from "./hr.module.css";

/** How many names a cell shows before it counts the rest. A day where the whole
 *  team is off must not make the row taller than the screen. */
const NAMES_PER_DAY = 3;

/** `2026-08` or nothing at all. A month this build cannot read is this month:
 *  an address somebody mistyped opens the calendar rather than an error. */
function pickMonth(raw: string | null): string {
  return raw !== null && /^\d{4}-(0[1-9]|1[0-2])$/.test(raw)
    ? raw
    : monthOf(browserToday());
}

export function AwayView() {
  const api = useHrApi();
  const [searchParams, setSearchParams] = useSearchParams();
  const month = pickMonth(searchParams.get("month"));

  const [days, setDays] = useState<HrAbsenceDay[]>([]);
  const [holidays, setHolidays] = useState<Map<string, string>>(new Map());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const weeks = useMemo(() => monthWeeks(month), [month]);
  const from = weeks[0]?.[0] ?? month;
  const to = weeks[weeks.length - 1]?.[6] ?? month;
  const today = browserToday();

  function goTo(next: string) {
    setSearchParams(
      (params) => {
        const updated = new URLSearchParams(params);
        if (next === monthOf(today)) updated.delete("month");
        else updated.set("month", next);
        return updated;
      },
      { replace: true },
    );
  }

  // The absence layer for the whole grid, spill days included: a Monday that
  // belongs to last month is still a day somebody is off on.
  useEffect(() => {
    let live = true;
    setLoading(true);
    api
      .absences(from, to)
      .then((answer) => {
        if (!live) return;
        setDays(answer);
        setError(null);
      })
      .catch((err: unknown) => {
        if (live) setError(hrMessage(err, strings.hrLoadFailed));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [api, from, to]);

  // The public holidays, one read per year the grid touches — so a January grid
  // marks the days on both sides of New Year rather than half of them. A tenant
  // that observes nothing answers nothing, and the grid simply marks nothing.
  useEffect(() => {
    let live = true;
    const years = yearsOf([from, to]);
    void Promise.all(
      years.map((year) => api.holidays(year).catch(() => [])),
    ).then((answers) => {
      if (live) setHolidays(holidayIndex(answers.flat()));
    });
    return () => {
      live = false;
    };
  }, [api, from, to]);

  const absent = useMemo(() => absenceIndex(days), [days]);
  /** How many distinct people are away at some point *in the month itself* —
   *  the spill days belong to the months either side of it. */
  const inMonth = useMemo(
    () =>
      peopleAway(
        days.filter((day) => day.day.startsWith(month)),
        null,
      ).length,
    [days, month],
  );
  const weekdays = weekdayNames();

  return (
    <div className={styles.page}>
      <Toolbar label={strings.hrAwayControls} className="px-5 pt-4">
        <div className={styles.monthNav}>
          <Button
            variant="ghost"
            size="sm"
            aria-label={strings.hrPreviousMonth}
            onClick={() => goTo(shiftMonth(month, -1))}
          >
            <ChevronLeft size={16} />
          </Button>
          <strong className={styles.monthName}>{monthLabel(month)}</strong>
          <Button
            variant="ghost"
            size="sm"
            aria-label={strings.hrNextMonth}
            onClick={() => goTo(shiftMonth(month, 1))}
          >
            <ChevronRight size={16} />
          </Button>
          {month !== monthOf(today) && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => goTo(monthOf(today))}
            >
              {strings.hrThisMonth}
            </Button>
          )}
        </div>

        <span className="flex-1" />
        {loading && <Spinner size={16} />}
        <span className={styles.muted}>{strings.hrAwayThisMonth(inMonth)}</span>
      </Toolbar>

      <div className={styles.inbox}>
        {error !== null && <ErrorBanner message={error} />}

        {!loading && error === null && inMonth === 0 && (
          <EmptyState
            Icon={CalendarDays}
            title={strings.hrNobodyAwayTitle(monthLabel(month))}
            body={strings.hrNobodyAwayBody}
          />
        )}

        <div
          className={styles.calendar}
          role="grid"
          aria-label={strings.hrAwayCalendar}
        >
          <div className={styles.calendarHead} role="row">
            {weekdays.map((name) => (
              <span key={name} className={styles.weekday} role="columnheader">
                {name}
              </span>
            ))}
          </div>
          {weeks.map((week) => (
            <div key={week[0]} className={styles.calendarWeek} role="row">
              {week.map((day) => {
                const people = absent.get(day) ?? [];
                const holiday = holidays.get(day);
                const classes = [styles.cell];
                if (!day.startsWith(month)) classes.push(styles.cellOutside);
                if (isWeekend(day)) classes.push(styles.cellWeekend);
                if (holiday !== undefined) classes.push(styles.cellHoliday);
                if (day === today) classes.push(styles.cellToday);
                return (
                  <div
                    key={day}
                    className={classes.join(" ")}
                    role="gridcell"
                    aria-label={strings.hrDayAway(day, people.length)}
                  >
                    <span className={styles.cellDay}>
                      {Number(day.slice(8))}
                    </span>
                    {holiday !== undefined && (
                      <span className={styles.cellNote}>{holiday}</span>
                    )}
                    {people.slice(0, NAMES_PER_DAY).map((person) => (
                      <span
                        key={person.employeeId}
                        className={styles.cellPerson}
                      >
                        {person.name}
                      </span>
                    ))}
                    {people.length > NAMES_PER_DAY && (
                      <span className={styles.cellMore}>
                        {strings.hrMoreAway(people.length - NAMES_PER_DAY)}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
