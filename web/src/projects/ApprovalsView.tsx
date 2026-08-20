// Approvals — the manager's inbox: every week somebody has handed in, oldest
// first, with the person, the period, the hours and the decision.
//
// This is the one screen in Projects that names a person, and the only one
// reached through the admin door. `docs/design/projects.md` § "The hours of a
// person are personal data" is why: per-user hours are visible to their owner
// and to a tenant admin, and to nobody else. The tab is **hidden entirely** for
// a caller who is not an admin rather than shown disabled — a control that
// exists only to refuse is a control that teaches nothing.
//
// A rejection carries a note, and it is asked for rather than optional in
// spirit: the person whose week comes back is going to read it, and "rejected"
// with no sentence is a manager making somebody guess. The server accepts an
// empty one; this screen prompts for it.
import { useEffect, useState } from "react";
import { ArrowRight, CheckCircle2, Inbox } from "lucide-react";

import { Button, Spinner, useDialogs } from "../ds";
import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import { dayLabel, durationLabel, momentLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import type { PendingProjectHours, PendingWeek } from "./types";

export function ApprovalsView({
  onDecided,
  onOpenProject,
}: {
  onDecided: () => void;
  onOpenProject: (projectId: string) => void;
}) {
  const api = useProjectsApi();
  const dialogs = useDialogs();
  const [weeks, setWeeks] = useState<PendingWeek[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [approvedProjects, setApprovedProjects] = useState<PendingProjectHours[]>([]);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const pending = await api.approvals();
        if (live) {
          setWeeks(pending);
          setError(null);
        }
      } catch (err) {
        if (live) setError(projectsMessage(err, strings.projectsLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, reload]);

  async function approve(week: PendingWeek) {
    setBusy(week.id);
    setError(null);
    try {
      await api.approveWeek(week.id);
      setApprovedProjects(week.projects);
      setWeeks((current) => current.filter((candidate) => candidate.id !== week.id));
      setReload((r) => r + 1);
      onDecided();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
    } finally {
      setBusy(null);
    }
  }

  async function reject(week: PendingWeek) {
    const note = await dialogs.prompt({
      title: strings.projectsRejectTitle,
      message: strings.projectsRejectBody(week.userEmail),
      confirmLabel: strings.projectsReject,
      placeholder: strings.projectsRejectPlaceholder,
    });
    // `null` is a cancelled prompt — not an empty note.
    if (note === null) return;
    setBusy(week.id);
    setError(null);
    try {
      await api.rejectWeek(week.id, note);
      setWeeks((current) => current.filter((candidate) => candidate.id !== week.id));
      setReload((r) => r + 1);
      onDecided();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsSaveFailed));
    } finally {
      setBusy(null);
    }
  }

  if (loading && weeks.length === 0) {
    return (
      <div className="flex min-h-0 flex-col gap-4 overflow-auto px-5 py-4">
        <Spinner size={20} />
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-col gap-4 overflow-auto px-5 py-4">
      {error !== null && <ErrorBanner message={error} />}
      {approvedProjects.length > 0 && (
        <section className="flex flex-wrap items-center gap-4 rounded-xl border border-success/30 bg-success/10 px-4 py-3" role="status">
          <CheckCircle2 className="size-5 shrink-0 text-success" aria-hidden="true" />
          <div className="min-w-52 flex-1">
            <p className="font-semibold text-primary">{strings.projectsApprovalComplete}</p>
            <p className="mt-0.5 text-sm text-secondary">{strings.projectsApprovalCompleteBody}</p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {approvedProjects.map((project) => (
              <button
                key={project.projectId}
                type="button"
                className="inline-flex min-h-10 items-center gap-2 rounded-lg bg-surface px-4 py-2 text-sm font-semibold text-primary shadow-sm ring-1 ring-inset ring-subtle !no-underline transition-colors hover:bg-raised hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                onClick={() => onOpenProject(project.projectId)}
              >
                <span className="max-w-48 truncate">{project.projectName}</span>
                <ArrowRight className="size-4 shrink-0" aria-hidden="true" />
              </button>
            ))}
          </div>
        </section>
      )}
      {weeks.length === 0 ? (
        <EmptyState
          Icon={Inbox}
          title={strings.projectsApprovalsEmptyTitle}
          body={strings.projectsApprovalsEmptyBody}
        />
      ) : (
        <div className="overflow-x-auto rounded-lg border border-subtle bg-surface">
          <table className="w-full border-collapse text-sm [&_th]:whitespace-nowrap [&_th]:border-b [&_th]:border-subtle [&_th]:px-3.5 [&_th]:py-2.5 [&_th]:text-left [&_th]:font-medium [&_th]:text-tertiary [&_td]:border-b [&_td]:border-subtle [&_td]:px-3.5 [&_td]:py-2.5 [&_td]:align-middle [&_td]:text-primary [&_tbody_tr:hover]:bg-raised">
            <thead>
              <tr>
                <th scope="col">{strings.projectsPerson}</th>
                <th scope="col">{strings.projectsWeek}</th>
                <th scope="col">{strings.projectsProject}</th>
                <th scope="col" className="whitespace-nowrap text-right tabular-nums">
                  {strings.projectsHoursLogged}
                </th>
                <th scope="col" className="whitespace-nowrap text-right tabular-nums">
                  {strings.projectsBillableHours}
                </th>
                <th scope="col">{strings.projectsSubmittedAt}</th>
                <th scope="col" aria-label={strings.projectsActions} />
              </tr>
            </thead>
            <tbody>
              {weeks.map((week) => (
                <tr key={week.id}>
                  <td>{week.userEmail}</td>
                  <td>
                    {strings.projectsWeekOf(
                      dayLabel(week.weekStart, { day: "numeric", month: "short" }),
                      dayLabel(week.weekEnd),
                    )}
                  </td>
                  <td>
                    <div className="flex min-w-48 flex-col gap-1.5">
                      {week.projects.map((project) => (
                        <div key={project.projectId} className="flex items-center justify-between gap-4 rounded-md bg-raised px-2.5 py-1.5">
                          <button
                            type="button"
                            className="min-w-0 truncate rounded text-left font-medium text-primary !no-underline hover:text-accent hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                            onClick={() => onOpenProject(project.projectId)}
                          >
                            {project.projectName}
                          </button>
                          <span className="shrink-0 text-xs tabular-nums text-secondary">
                            {durationLabel(project.minutes)}
                            {project.billableMinutes > 0 && ` · ${durationLabel(project.billableMinutes)} ${strings.projectsBillableHours.toLocaleLowerCase()}`}
                          </span>
                        </div>
                      ))}
                    </div>
                  </td>
                  <td className="whitespace-nowrap text-right tabular-nums">{durationLabel(week.minutes)}</td>
                  <td className="whitespace-nowrap text-right tabular-nums">{durationLabel(week.billableMinutes)}</td>
                  <td className="text-tertiary">
                    {week.submittedAt === null ? "" : momentLabel(week.submittedAt)}
                  </td>
                  <td>
                    <div className="flex items-center justify-end gap-2">
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={busy !== null}
                        onClick={() => void reject(week)}
                      >
                        {strings.projectsReject}
                      </Button>
                      <Button
                        size="sm"
                        disabled={busy !== null}
                        onClick={() => void approve(week)}
                      >
                        {strings.projectsApprove}
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
