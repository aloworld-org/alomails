// Which approval queues are this caller's to work (alo HR, ADR 0035, wave
// B6.07) — resolved once, in one place, and used by both the tab that draws the
// inbox and the widget that counts it.
//
// Three doors, and they are not the same door — which is exactly why
// `docs/design/hr.md` § "The approvals inbox" refused a server-side `/approvals`
// that would have had to hold all three in one handler:
//
//   | queue      | who may decide                        | asked as             |
//   |------------|---------------------------------------|----------------------|
//   | leave      | HR, or anybody with a direct report    | `canWorkHr` + the org |
//   | expenses   | tenant admin or accountant             | `canWorkTheBooks`    |
//   | timesheets | tenant admin                           | `isAdmin`            |
//
// **None of this is an access decision.** It decides what to *draw*: every
// route behind these queues asks its own door again and answers `403`, so a
// stale session hides a queue at worst and opens nothing at all. The reverse —
// a queue drawn for somebody who may not work it — is the failure this resolver
// exists to avoid: a control that exists only to refuse teaches nothing
// (`docs/design/ux-principles.md`).
//
// The manager case is the one that costs a read. Nothing on the session says
// "somebody reports to you", so for a caller who is not HR the resolver asks
// `/hr/me` and the directory — both every member's reads — and looks for one
// row whose `managerId` is theirs. It stops at the first: the question is
// whether they manage *anybody*, not how many.
import { useEffect, useMemo, useState } from "react";

import type { ApprovalQueue } from "../platform/approvals";
import { useAuth } from "../auth";
import { useExpenseApprovals } from "../finance";
import { useWeekApprovals } from "../projects";
import { useJmapClient } from "../jmap";
import { useHrApi } from "./api";
import { useLeaveApprovals } from "./leaveApprovals";

/** What the doors said, before it is turned into a list of queues. */
interface Access {
  /** HR sees the tenant's leave; a manager sees their reports'. */
  leave: "all" | "team" | null;
  expenses: boolean;
  timesheets: boolean;
}

/** Nothing yet — the honest state before the session and the org have
 *  answered, and never the state a tab is drawn from. */
const CLOSED: Access = { leave: null, expenses: false, timesheets: false };

/**
 * The answer, once per session.
 *
 * The resolver runs in three places at once — the module deciding its tabs, the
 * inbox loading its rows, the badge in the rail counting them — and the manager
 * case costs two HTTP reads. Asking three times for an answer that cannot
 * differ is three times the cost and, worse, three chances for the tab and the
 * badge to disagree while they resolve.
 *
 * Keyed on the session's own authorized fetch — the one object in this app that
 * is exactly one per signed-in person. A new session is a new function and
 * therefore a new answer, so this can never serve one person's doors to the
 * next, and signing out drops the entry with the context that held it.
 */
const RESOLVED = new WeakMap<object, Promise<Access>>();

/**
 * The approval queues this caller may work.
 *
 * `ready` is false until every door has answered. A caller with no door gets an
 * empty list, which is what hides the inbox from somebody who has nothing to
 * decide — a state most members are in, and not a refusal.
 */
export function useApprovalQueues(): { ready: boolean; queues: ApprovalQueue[] } {
  const access = useAccess();

  // The three hooks are called unconditionally — a queue that is not this
  // caller's is simply left out of the list below, never left uncreated.
  const leave = useLeaveApprovals(access?.leave === "all" ? "all" : "team");
  const expenses = useExpenseApprovals();
  const weeks = useWeekApprovals();

  return useMemo(() => {
    const doors = access ?? CLOSED;
    const queues: ApprovalQueue[] = [];
    if (doors.leave !== null) queues.push(leave);
    if (doors.expenses) queues.push(expenses);
    if (doors.timesheets) queues.push(weeks);
    return { ready: access !== null, queues };
  }, [access, leave, expenses, weeks]);
}

/**
 * Whose leave this caller may read beyond their own: `all` for HR, `team` for
 * somebody with a direct report, `null` for most members.
 *
 * The leave screen's scope switch, and the same answer the inbox's leave queue
 * is built from — resolved once and shared, so the switch and the queue can
 * never disagree about who the reader is. Like the queues above it decides only
 * what to *draw*: `hr_leave_door.rs` resolves the scope again for every read and
 * refuses `all` from anybody who is not HR.
 */
export function useLeaveScope(): { ready: boolean; scope: "all" | "team" | null } {
  const access = useAccess();
  return useMemo(
    () => ({ ready: access !== null, scope: access?.leave ?? null }),
    [access],
  );
}

/** The doors, once per session. `null` until they have all answered — and never
 *  a guess in the meantime, because a tab drawn from a guess flashes somebody
 *  else's screen at a reader on every load. */
function useAccess(): Access | null {
  const { authorizedFetch } = useAuth();
  const client = useJmapClient();
  const hr = useHrApi();
  const [access, setAccess] = useState<Access | null>(null);

  useEffect(() => {
    let live = true;
    let answer = RESOLVED.get(authorizedFetch);
    if (answer === undefined) {
      answer = resolve(client, hr);
      RESOLVED.set(authorizedFetch, answer);
    }
    void answer.then((resolved) => {
      if (live) setAccess(resolved);
    });
    return () => {
      live = false;
    };
  }, [authorizedFetch, client, hr]);

  return access;
}

/** Asks the three doors. A door that cannot be asked is a door that is shut:
 *  an unreachable surface must never open a queue. */
async function resolve(
  client: ReturnType<typeof useJmapClient>,
  hr: ReturnType<typeof useHrApi>,
): Promise<Access> {
  const [isHr, books, admin] = await Promise.all([
    client.canWorkHr().catch(() => false),
    client.canWorkTheBooks().catch(() => false),
    client.isAdmin().catch(() => false),
  ]);
  return {
    leave: isHr ? "all" : (await managesSomebody(hr)) ? "team" : null,
    expenses: books,
    timesheets: admin,
  };
}

/** Whether anybody in this tenant reports to the caller. Two every-member
 *  reads, and only for a caller who is not HR — HR already has the wider
 *  answer and needs neither. */
async function managesSomebody(hr: ReturnType<typeof useHrApi>): Promise<boolean> {
  try {
    const mine = (await hr.me()).employee?.id;
    if (mine === undefined || mine === "") return false;
    const directory = await hr.directory();
    return directory.employees.some((entry) => entry.managerId === mine && !entry.archived);
  } catch {
    // No employee record, an HR surface that is not there, a dropped
    // connection: none of them is somebody's approver.
    return false;
  }
}
