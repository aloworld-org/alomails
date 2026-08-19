// The directory (alo HR, ADR 0035, wave B6.08a): who works here, how to reach
// them, and who reports to whom — the first HR screen every member of a tenant
// can open.
//
// It is deliberately the module's plainest screen and its most widely read one.
// A company where finding a colleague's team means asking a person has its org
// chart in a filing cabinet, and this suite exists to close filing cabinets. So:
//
//   - **Every member gets it, and it holds the public fields only.** Not because
//     this screen filters anything — because the route it reads answers a
//     projection that has no home address on it to leak. HR's read differs by
//     exactly one thing, the people who have left, and the control for that is
//     drawn only for HR (`hr` on the answer) because a control that exists to
//     be ignored teaches the wrong thing.
//   - **One search box over both views.** The list narrows to the people who
//     match; the chart keeps every branch that matches *and the managers above
//     it*, so a narrowed chart still says who somebody works for
//     (`directory.ts`).
//   - **What is on screen is in the address** — the view, the search and the
//     person the chart is showing — so a colleague's place in the company is a
//     link somebody can send, and a reload lands back on it.
//
// No record is written here. The one act this screen offers besides looking is
// pressing a person in the list to see where they sit in the chart.
import { useCallback, useEffect, useMemo, useState } from "react";
import { Network, Search, Users } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import { Avatar, Checkbox, Spinner, Table, Td, Th, Toolbar } from "../ds";
import { strings } from "../i18n";
import { hrMessage, useHrApi } from "./api";
import {
  byId,
  countOrg,
  filterDirectory,
  filterOrg,
  managerName,
} from "./directory";
import { dayLabel } from "./format";
import { OrgChart } from "./OrgChart";
import { EmptyState, ErrorBanner, StateBadge } from "./parts";
import type { HrDirectoryEntry, HrOrgNode } from "./types";
import styles from "./hr.module.css";

/** Which of the two readings of the same people is on screen. */
type View = "people" | "org";

export function DirectoryView() {
  const api = useHrApi();
  const [searchParams, setSearchParams] = useSearchParams();
  const [entries, setEntries] = useState<HrDirectoryEntry[]>([]);
  const [chart, setChart] = useState<HrOrgNode[] | null>(null);
  const [isHr, setIsHr] = useState(false);
  const [selfId, setSelfId] = useState<string | null>(null);
  const [includeArchived, setIncludeArchived] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const view: View = searchParams.get("view") === "org" ? "org" : "people";
  const query = searchParams.get("q") ?? "";
  const person = searchParams.get("person");

  /**
   * Puts keys in the address, or takes them out. Replace, not push: reading a
   * directory is not a trail of history entries to press Back through.
   *
   * Several keys in **one** call, deliberately: two calls in one event each
   * build on the address as it is committed, so the second would drop what the
   * first put there — which is exactly the pair "show this person, in the
   * chart".
   */
  const setParams = useCallback(
    (changes: Record<string, string | null>) => {
      setSearchParams(
        (params) => {
          const next = new URLSearchParams(params);
          for (const [key, value] of Object.entries(changes)) {
            if (value === null || value === "") next.delete(key);
            else next.set(key, value);
          }
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

  // The people. Re-read when the archived question changes, because the answer
  // is a different set of rows and not a filter this screen could apply.
  useEffect(() => {
    let live = true;
    setLoading(true);
    api
      .directory(includeArchived)
      .then((answer) => {
        if (!live) return;
        setEntries(answer.employees);
        setIsHr(answer.hr);
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
  }, [api, includeArchived]);

  // Who the reader is, so their own row is marked. A login with no employee
  // record — a contractor with a mailbox — is an ordinary answer: nobody is
  // marked, and the screen is the same screen.
  useEffect(() => {
    let live = true;
    api
      .me()
      .then((me) => {
        if (live) setSelfId(me.employee?.id ?? null);
      })
      .catch(() => {
        // Not knowing which row is yours costs a highlight, not a screen.
        if (live) setSelfId(null);
      });
    return () => {
      live = false;
    };
  }, [api]);

  // The chart, read the first time it is asked for and kept. It does not vary
  // with the archived question — `/hr/org` is the people who are here — so
  // toggling views never asks twice.
  useEffect(() => {
    if (view !== "org" || chart !== null) return;
    let live = true;
    api
      .orgChart()
      .then((tree) => {
        if (live) setChart(tree);
      })
      .catch((err: unknown) => {
        if (live) setError(hrMessage(err, strings.hrLoadFailed));
      });
    return () => {
      live = false;
    };
  }, [api, view, chart]);

  const index = useMemo(() => byId(entries), [entries]);
  const shown = useMemo(
    () => filterDirectory(entries, query),
    [entries, query],
  );
  const shownChart = useMemo(
    () => filterOrg(chart ?? [], query),
    [chart, query],
  );

  const total = view === "org" ? countOrg(chart ?? []) : entries.length;
  const matching = view === "org" ? countOrg(shownChart) : shown.length;
  const searching = query.trim() !== "";

  return (
    <div className={styles.page}>
      <Toolbar
        label={strings.hrDirectoryControls}
        align="end"
        className="px-5 pt-4"
      >
        <label className={styles.search}>
          <Search size={15} aria-hidden="true" />
          <input
            className={styles.searchInput}
            type="search"
            value={query}
            placeholder={strings.hrDirectorySearch}
            aria-label={strings.hrDirectorySearch}
            onChange={(e) => setParams({ q: e.target.value })}
          />
        </label>

        <div
          className={styles.segmented}
          role="group"
          aria-label={strings.hrDirectoryViews}
        >
          <button
            type="button"
            className={view === "people" ? styles.segmentOn : styles.segment}
            aria-pressed={view === "people"}
            onClick={() => setParams({ view: null, person: null })}
          >
            <Users size={14} aria-hidden="true" />
            {strings.hrViewPeople}
          </button>
          <button
            type="button"
            className={view === "org" ? styles.segmentOn : styles.segment}
            aria-pressed={view === "org"}
            onClick={() => setParams({ view: "org" })}
          >
            <Network size={14} aria-hidden="true" />
            {strings.hrViewOrg}
          </button>
        </div>

        {/* HR's one wider read. Hidden rather than disabled for everybody else:
            the route ignores the flag for them, so the control would change
            nothing and say something untrue about what they can see. */}
        {isHr && view === "people" && (
          <Checkbox
            checked={includeArchived}
            onChange={setIncludeArchived}
            label={strings.hrIncludeLeavers}
          />
        )}

        <span className="flex-1" />
        {loading && <Spinner size={16} />}
        <span className={styles.muted}>
          {searching
            ? strings.hrShowingOf(matching, total)
            : strings.hrPeopleCount(total)}
        </span>
      </Toolbar>

      <div className={styles.directoryBody}>
        {error !== null && <ErrorBanner message={error} />}

        {!loading && entries.length === 0 && error === null ? (
          <EmptyState
            Icon={Users}
            title={strings.hrDirectoryEmptyTitle}
            body={strings.hrDirectoryEmptyBody}
          />
        ) : view === "org" && chart === null ? (
          <div className={styles.orgLoading}>
            <Spinner size={20} />
          </div>
        ) : matching === 0 && searching ? (
          <EmptyState
            Icon={Search}
            title={strings.hrNoMatchTitle(query)}
            body={strings.hrNoMatchBody}
            cta={strings.hrClearSearch}
            onCta={() => setParams({ q: null })}
          />
        ) : view === "people" ? (
          <PeopleTable
            entries={shown}
            index={index}
            selfId={selfId}
            onShowInChart={(id) => setParams({ person: id, view: "org" })}
          />
        ) : (
          <OrgChart nodes={shownChart} highlightId={person} selfId={selfId} />
        )}
      </div>
    </div>
  );
}

/** The people, as a table. Contact details are the two the record holds, shown
 *  as the actions they are: an address opens a message, a number dials. */
function PeopleTable({
  entries,
  index,
  selfId,
  onShowInChart,
}: {
  entries: HrDirectoryEntry[];
  index: Map<string, HrDirectoryEntry>;
  selfId: string | null;
  onShowInChart: (id: string) => void;
}) {
  return (
    <Table label={strings.hrDirectoryTable}>
      <thead>
        <tr>
          <Th>{strings.hrPerson}</Th>
          <Th>{strings.hrFieldRole}</Th>
          <Th>{strings.hrFieldTeam}</Th>
          <Th>{strings.hrContact}</Th>
          <Th>{strings.hrManager}</Th>
          <Th>{strings.hrSince}</Th>
          <Th hideLabel>{strings.hrDirectoryViews}</Th>
        </tr>
      </thead>
      <tbody>
        {entries.map((entry) => {
          const manager = managerName(entry, index);
          return (
            <tr key={entry.id}>
              <Td>
                <span className={styles.personCell}>
                  <Avatar name={entry.name} email={entry.workEmail} size="sm" />
                  <span className={styles.personName}>
                    {/* The name in an element of its own, never mixed with
                          the tag beside it: it is what a reader scans for. */}
                    <span>{entry.name}</span>
                    {entry.id === selfId && (
                      <span className={styles.selfTag}>{strings.hrYou}</span>
                    )}
                  </span>
                  {entry.archived && (
                    <StateBadge tone="bad">{strings.hrLeft}</StateBadge>
                  )}
                </span>
              </Td>
              <Td>{entry.jobTitle}</Td>
              <Td>{entry.team}</Td>
              <Td>
                {entry.workEmail !== "" && (
                  <a
                    className={styles.rowLink}
                    href={`mailto:${entry.workEmail}`}
                  >
                    {entry.workEmail}
                  </a>
                )}
                {entry.workPhone !== "" && (
                  <a className={styles.subtle} href={`tel:${entry.workPhone}`}>
                    {entry.workPhone}
                  </a>
                )}
              </Td>
              <Td className={manager === null ? styles.muted : undefined}>
                {manager ?? "—"}
              </Td>
              <Td className={styles.muted}>{dayLabel(entry.startedOn)}</Td>
              <Td>
                {/* Only for people the chart actually holds: it is the active
                      tenant, so a colleague who has left has no place in it. */}
                {!entry.archived && (
                  <button
                    type="button"
                    className={styles.linkAction}
                    onClick={() => onShowInChart(entry.id)}
                  >
                    {strings.hrShowInChart}
                  </button>
                )}
              </Td>
            </tr>
          );
        })}
      </tbody>
    </Table>
  );
}
