// Time off (alo HR, ADR 0035, wave B6.08b): what somebody has left, what they
// have asked for, and — for whoever decides it — the same list one relationship
// wider.
//
// It is one screen and not two on purpose. A manager asking for their own week
// off and approving their report's is the same subject looked at from two
// sides, and the module already has a place where decisions are *collected*
// (the approvals inbox, B6.07). What that inbox cannot give is context: it
// shows one line per waiting thing, deliberately, because it holds three kinds
// at once. This screen is where a leave row leads from there — the same
// decision with the dates, the working behind the days, the person's own
// sentence, and what else their team already has booked.
//
// Four rules the screen keeps:
//
//   - **No figure is computed here.** The balance and the cost of a request
//     arrive from the server with their working, folded over the person's
//     working pattern and the tenant's public holidays (`leave.ts`).
//   - **The clock is the server's.** `/hr/leave-balances` echoes the day it
//     folded to, and that day decides whether an absence has already begun —
//     the same calendar the refusal would come from.
//   - **The scope is a question, not a filter.** `mine`, `team` and `all` are
//     three server-side reads naming three different sets of people; this
//     screen never reads everybody's and narrows afterwards, which is the shape
//     that leaks the day somebody edits the filter.
//   - **Approving is not confirmed** (`docs/design/ux-principles.md`, undo over
//     confirm): cancelling gives the balance back. Sending back *is* asked for
//     a sentence, because the person is going to read it.
//
// What is on screen is in the address — the scope, the filter and the request
// somebody arrived for — so the inbox can link to a row and a reload lands back
// on it.
import { useCallback, useEffect, useMemo, useState } from "react";
import { CalendarOff, CalendarPlus, Inbox as InboxIcon } from "lucide-react";
import { useSearchParams } from "react-router-dom";

import {
  Button,
  Card,
  Select,
  Spinner,
  Table,
  Td,
  Th,
  Toolbar,
  useDialogs,
} from "../ds";
import { strings } from "../i18n";
import { hrMessage, useHrApi } from "./api";
import { announceApprovalsChanged } from "./approvalsBus";
import { dayLabel, momentLabel } from "./format";
import { LeaveDialog } from "./LeaveDialog";
import {
  browserToday,
  canCancel,
  canDecide,
  canWithdraw,
  daysLabel,
  leaveStatusLabel,
  leaveStatusTone,
} from "./leave";
import { EmptyState, ErrorBanner, StateBadge } from "./parts";
import { useLeaveScope } from "./queues";
import type {
  HrLeaveBalances,
  HrLeaveRequest,
  HrPolicyBalance,
  LeaveStatus,
} from "./types";
import styles from "./hr.module.css";

/** Whose leave is on screen. Three server-side questions, never a filter. */
type Scope = "mine" | "team" | "all";

/** Which states the list is narrowed to. `all` is every state, because the
 *  record of a refused week is part of what a person came to read. */
type Show = "all" | "waiting" | "booked";

const SHOWN: Record<Show, LeaveStatus[]> = {
  all: [],
  waiting: ["requested"],
  booked: ["approved"],
};

/** The scope actually asked for: what the address says, narrowed to what this
 *  caller's door allows. A stale link to `all` from somebody who has since lost
 *  the HR role reads their own leave rather than collecting a `403`. */
function pickScope(raw: string | null, door: "all" | "team" | null): Scope {
  if (raw === "all" && door === "all") return "all";
  if (raw === "team" && door !== null) return "team";
  return "mine";
}

export function LeaveView() {
  const api = useHrApi();
  const dialogs = useDialogs();
  const door = useLeaveScope();
  const [searchParams, setSearchParams] = useSearchParams();

  const scope = pickScope(searchParams.get("scope"), door.scope);
  const show: Show = ((): Show => {
    const raw = searchParams.get("show");
    return raw === "waiting" || raw === "booked" ? raw : "all";
  })();
  const marked = searchParams.get("request");

  const [balances, setBalances] = useState<HrLeaveBalances | null>(null);
  const [noRecord, setNoRecord] = useState<string | null>(null);
  const [requests, setRequests] = useState<HrLeaveRequest[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  const reload = useCallback(() => setRevision((r) => r + 1), []);

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

  // The caller's own balance — and with it the two facts the rest of the screen
  // needs: which employee record is theirs, and what day the server thinks it
  // is. A login with no employee record is an ordinary answer here (`409`), not
  // a broken screen: the server's sentence says whose job it is to fix.
  useEffect(() => {
    let live = true;
    api
      .leaveBalances()
      .then((answer) => {
        if (!live) return;
        setBalances(answer);
        setNoRecord(null);
      })
      .catch((err: unknown) => {
        if (live) setNoRecord(hrMessage(err, strings.hrLoadFailed));
      });
    return () => {
      live = false;
    };
  }, [api, revision]);

  // The requests. Not asked for until the door has answered: `team` and `all`
  // are questions this caller may not be allowed to put, and asking early would
  // collect a `403` on the way to the right question.
  useEffect(() => {
    if (!door.ready) return undefined;
    let live = true;
    setLoading(true);
    api
      .leaveRequests(scope, SHOWN[show])
      .then((rows) => {
        if (!live) return;
        setRequests(rows);
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
  }, [api, door.ready, scope, show, revision]);

  const me = balances?.employeeId ?? null;
  // The server's day when there is one, this device's when there is not — and
  // either way the act itself is re-checked against the server's calendar.
  const today =
    balances !== null && balances.on !== "" ? balances.on : browserToday();
  /** The policies somebody may ask on: the live ones the balance read named.
   *  A retired policy has a balance and no future — it is in the cards and not
   *  in the picker. */
  const policies = useMemo(
    () =>
      (balances?.balances ?? [])
        .map((entry) => entry.policy)
        .filter((p) => !p.archived),
    [balances],
  );

  /** One act, taken and then re-read. Nothing on this screen edits a row from
   *  what it sent: the server's answer is the record, and a decision anywhere
   *  is a count the rail has to re-read too. */
  async function act(id: string, verb: () => Promise<unknown>) {
    setBusy(id);
    setError(null);
    try {
      await verb();
      announceApprovalsChanged();
      reload();
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(null);
    }
  }

  async function sendBack(request: HrLeaveRequest) {
    const written = await dialogs.prompt({
      title: strings.hrSendBackTitle,
      message: strings.hrSendBackBody(request.employeeName),
      confirmLabel: strings.hrSendBack,
      placeholder: strings.hrSendBackPlaceholder,
    });
    // `null` is a cancelled prompt — not an empty note.
    if (written === null) return;
    await act(request.id, () => api.rejectLeaveRequest(request.id, written));
  }

  const showingPerson = scope !== "mine";
  const settled = door.ready && !loading;

  return (
    <div className={styles.page}>
      <Toolbar
        label={strings.hrLeaveControls}
        align="end"
        className="px-5 pt-4"
      >
        {/* Drawn only for somebody who has a second question to ask. Most
            members have one, and a switch with a single position is furniture. */}
        {door.scope !== null && (
          <div
            className={styles.segmented}
            role="group"
            aria-label={strings.hrLeaveWhose}
          >
            <button
              type="button"
              className={scope === "mine" ? styles.segmentOn : styles.segment}
              aria-pressed={scope === "mine"}
              onClick={() => setParams({ scope: null, request: null })}
            >
              {strings.hrScopeMine}
            </button>
            <button
              type="button"
              className={scope === "team" ? styles.segmentOn : styles.segment}
              aria-pressed={scope === "team"}
              onClick={() => setParams({ scope: "team", request: null })}
            >
              {strings.hrScopeTeam}
            </button>
            {door.scope === "all" && (
              <button
                type="button"
                className={scope === "all" ? styles.segmentOn : styles.segment}
                aria-pressed={scope === "all"}
                onClick={() => setParams({ scope: "all", request: null })}
              >
                {strings.hrScopeEveryone}
              </button>
            )}
          </div>
        )}

        {/* A `<label>` around the control names it, which is the one thing a
            bare filter in a toolbar usually lacks. */}
        <label className="inline-flex items-center gap-2 text-sm text-secondary">
          {strings.hrLeaveShow}
          <Select
            value={show}
            onChange={(e) =>
              setParams({
                show: e.target.value === "all" ? null : e.target.value,
              })
            }
          >
            <option value="all">{strings.hrShowEverything}</option>
            <option value="waiting">{strings.hrShowWaiting}</option>
            <option value="booked">{strings.hrShowBooked}</option>
          </Select>
        </label>

        <span className="flex-1" />
        {(!settled || busy !== null) && <Spinner size={16} />}
        {/* Asking is only offered to somebody the tenant has a record for: the
            server refuses the rest with a sentence about linking the record,
            and a button that exists to collect that sentence teaches nothing. */}
        {balances !== null && (
          <Button onClick={() => setAsking(true)}>
            {strings.hrAskForLeave}
          </Button>
        )}
      </Toolbar>

      <div className={styles.inbox}>
        {error !== null && <ErrorBanner message={error} />}

        {/* The caller's own standing, always their own — a manager reading their
            team's requests is still the person whose holiday it is. */}
        {noRecord !== null && <p className={styles.notice}>{noRecord}</p>}
        {balances !== null && (
          <>
            <div className={styles.balances}>
              {balances.balances.map((entry) => (
                <BalanceCard key={entry.policy.id} entry={entry} />
              ))}
            </div>
            <p className={styles.subtle}>
              {strings.hrBalanceAsOf(dayLabel(balances.on))}
            </p>
          </>
        )}

        {!settled && requests.length === 0 ? (
          <div className={styles.orgLoading}>
            <Spinner size={20} />
          </div>
        ) : requests.length === 0 ? (
          show !== "all" ? (
            <EmptyState
              Icon={InboxIcon}
              title={strings.hrLeaveNoneShownTitle}
              body={strings.hrLeaveNoneShownBody}
              cta={strings.hrShowEverything}
              onCta={() => setParams({ show: null })}
            />
          ) : scope === "mine" ? (
            <EmptyState
              Icon={CalendarPlus}
              title={strings.hrLeaveEmptyTitle}
              body={strings.hrLeaveEmptyBody}
              {...(balances !== null
                ? { cta: strings.hrAskForLeave, onCta: () => setAsking(true) }
                : {})}
            />
          ) : (
            <EmptyState
              Icon={CalendarOff}
              title={strings.hrLeaveTeamEmptyTitle}
              body={strings.hrLeaveTeamEmptyBody}
            />
          )
        ) : (
          <Table label={strings.hrLeaveTable}>
            <thead>
              <tr>
                {showingPerson && <Th>{strings.hrPerson}</Th>}
                <Th>{strings.hrLeaveKind}</Th>
                <Th>{strings.hrLeaveWhen}</Th>
                <Th numeric>{strings.hrLeaveDays}</Th>
                <Th>{strings.hrLeaveWhy}</Th>
                <Th>{strings.hrLeaveState}</Th>
                <Th hideLabel>{strings.hrActions}</Th>
              </tr>
            </thead>
            <tbody>
              {requests.map((request) => (
                <tr
                  key={request.id}
                  className={
                    request.id === marked ? styles.rowMarked : undefined
                  }
                  aria-current={request.id === marked ? "true" : undefined}
                >
                  {showingPerson && <Td>{request.employeeName}</Td>}
                  <Td>{request.policyName}</Td>
                  <Td>
                    <span>
                      {strings.hrLeaveBetween(
                        dayLabel(request.fromDay),
                        dayLabel(request.toDay),
                      )}
                    </span>
                    {/* Why a week can cost four days. The figure is the
                          server's; this line is why it is not five. */}
                    {request.holidayMinutes > 0 && (
                      <span className={styles.subtle}>
                        {strings.hrHolidaysInside}
                      </span>
                    )}
                  </Td>
                  <Td numeric>{strings.hrWorkingDays(request.workingDays)}</Td>
                  <Td className={styles.subtle}>{request.note}</Td>
                  <Td>
                    <StateBadge tone={leaveStatusTone(request.status)}>
                      {leaveStatusLabel(request.status)}
                    </StateBadge>
                    {request.decidedAt !== null && (
                      <span className={styles.subtle}>
                        {momentLabel(request.decidedAt)}
                      </span>
                    )}
                    {request.decisionNote !== "" && (
                      <span className={styles.subtle}>
                        {request.decisionNote}
                      </span>
                    )}
                  </Td>
                  <Td>
                    <div className={styles.rowActions}>
                      {canWithdraw(request, me) && (
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={busy !== null}
                          onClick={() =>
                            void act(request.id, () =>
                              api.withdrawLeaveRequest(request.id),
                            )
                          }
                        >
                          {strings.hrWithdraw}
                        </Button>
                      )}
                      {canCancel(request, today) && (
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={busy !== null}
                          onClick={() =>
                            void act(request.id, () =>
                              api.cancelLeaveRequest(request.id),
                            )
                          }
                        >
                          {strings.hrCancelLeave}
                        </Button>
                      )}
                      {showingPerson && canDecide(request, me) && (
                        <>
                          <Button
                            variant="ghost"
                            size="sm"
                            disabled={busy !== null}
                            onClick={() => void sendBack(request)}
                          >
                            {strings.hrSendBack}
                          </Button>
                          <Button
                            size="sm"
                            disabled={busy !== null}
                            onClick={() =>
                              void act(request.id, () =>
                                api.approveLeaveRequest(request.id),
                              )
                            }
                          >
                            {strings.hrApprove}
                          </Button>
                        </>
                      )}
                    </div>
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </div>

      {asking && (
        <LeaveDialog
          policies={policies}
          onClose={() => setAsking(false)}
          onAsked={() => {
            setAsking(false);
            announceApprovalsChanged();
            reload();
          }}
        />
      )}
    </div>
  );
}

/** One policy's standing, with the working under it. The remaining figure is
 *  the one a person came for; the four behind it are what makes it believable
 *  (`docs/design/hr.md`: a balance nobody can reproduce is a support
 *  conversation). */
function BalanceCard({ entry }: { entry: HrPolicyBalance }) {
  return (
    <Card
      flat
      pad="none"
      className="flex flex-col gap-0.5 min-w-[200px] px-4 py-3"
    >
      <div className={styles.balanceHead}>
        <span className={styles.balanceName}>{entry.policy.name}</span>
        {!entry.policy.paid && (
          <StateBadge tone="info">{strings.hrUnpaid}</StateBadge>
        )}
        {!entry.policy.requiresApproval && (
          <StateBadge tone="info">{strings.hrNotDecided}</StateBadge>
        )}
      </div>
      <strong className={styles.balanceFigure}>
        {daysLabel(entry.remainingDaysTenths)}
      </strong>
      <span className={styles.balanceLabel}>{strings.hrBalanceLeft}</span>
      <div className={styles.balanceWorking}>
        <span>
          {strings.hrFactOf(
            strings.hrBalanceThisYear,
            daysLabel(entry.entitlementDaysTenths),
          )}
        </span>
        <span>
          {strings.hrFactOf(
            strings.hrBalanceTaken,
            daysLabel(entry.takenDaysTenths),
          )}
        </span>
        <span>
          {strings.hrFactOf(
            strings.hrBalanceBooked,
            daysLabel(entry.bookedDaysTenths),
          )}
        </span>
        <span>
          {strings.hrFactOf(
            strings.hrBalanceWaiting,
            daysLabel(entry.pendingDaysTenths),
          )}
        </span>
      </div>
    </Card>
  );
}
