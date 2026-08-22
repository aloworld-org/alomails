// The Projects module (alo Projects, ADR 0035, wave B3) — the workspace surface
// over the `/projects` API: the engagements a business works, the week one
// person fills in, and the weeks a manager decides about.
//
// It is mounted at `/projects/*` by the product surface, so every path below is
// relative and a deep link survives a page reload — including the running
// timer's own link, which is what the rail widget points at.
//
// The engagement list is loaded here, with the module, because every screen
// needs it: the week grid's rows are projects, the timer starts on one, and the
// list itself is the module's home. One `revision` counter ties them together —
// an hour written anywhere bumps it and whatever is on screen re-reads.
//
// **Approvals is hidden, not disabled, for a non-admin.** Per-user hours are
// visible to their owner and to a tenant admin and to nobody else
// (`docs/design/projects.md`), so a tab that exists only to refuse would be
// advertising a door this person does not have.
//
// A naming honesty note, from the design: this rail entry reads "Projects"
// while Tasks also calls its boards projects. They are the same rows, which is
// the point — so the copy here says *client project* wherever the distinction
// carries weight, and the Tasks module's own strings are left alone.
import { useCallback, useEffect, useState } from "react";
import { NavLink, Navigate, Route, Routes, useLocation, useNavigate, useParams, useSearchParams } from "react-router-dom";

import { useCustomers } from "../billing";
import { Spinner } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { TasksModule } from "../tasks";
import { ApprovalsView } from "./ApprovalsView";
import { ClientDialog } from "./ClientDialog";
import { EditProjectDialog } from "./EditProjectDialog";
import { InvoiceHandoffDialog } from "./InvoiceHandoffDialog";
import { NewProjectDialog } from "./NewProjectDialog";
import type { NewProjectDraft } from "./NewProjectDialog";
import { projectsMessage, useProjectsApi } from "./api";
import { ErrorBanner } from "./parts";
import { PlanView } from "./PlanView";
import { ProjectsView } from "./ProjectsView";
import { ReportView } from "./ReportView";
import {
  projectContextId,
  projectScopedPath,
  projectWorkspaceStatus,
  resolveProjectScope,
  shouldRemoveProjectScope,
} from "./scope";
import { TemplateDialog } from "./TemplateDialog";
import { announceTimerChanged, onTimerChanged } from "./timerBus";
import { WeekView } from "./WeekView";
import type { Project, ProjectDraft, ProjectTemplate, RunningTimer } from "./types";

/** Today as `YYYY-MM-DD` in the reader's own zone — the day a new project
 *  starts on unless they say otherwise. Local, not UTC: "today" is a fact about
 *  where the person is. */
function today(): string {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

function ProjectWorkspaceRoute({
  projects,
  projectsLoading,
  projectsLoadFailed,
}: {
  projects: Project[];
  projectsLoading: boolean;
  projectsLoadFailed: boolean;
}) {
  const { projectId, workspaceView } = useParams<{
    projectId: string;
    workspaceView: string;
  }>();
  if (projectId === undefined) return <Navigate to="/projects/list" replace />;
  const status = projectWorkspaceStatus(
    projectId,
    projectsLoading,
    projectsLoadFailed,
    projects,
  );
  if (status === "loading") {
    return (
      <div className="flex min-h-[28rem] items-center justify-center" role="status">
        <Spinner size={24} />
      </div>
    );
  }
  if (status === "missing") return <Navigate to="/projects/list" replace />;
  if (status === "unavailable") return null;
  return workspaceView === undefined
    ? <TasksModule projectId={projectId} />
    : <TasksModule projectId={projectId} workspaceView={workspaceView} />;
}

const projectTabClass = ({ isActive }: { isActive: boolean }) =>
  `inline-flex min-h-11 shrink-0 items-center rounded-t-lg border-b-2 px-4 py-2.5 text-sm !no-underline transition-colors hover:!no-underline focus:!no-underline active:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
    isActive
      ? "border-accent bg-[var(--accent-soft)] font-semibold !text-accent"
      : "border-transparent bg-transparent font-medium !text-secondary hover:bg-raised hover:!text-primary"
  }`;

export function ProjectsModule() {
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();
  const api = useProjectsApi();
  const client = useJmapClient();
  // Archived customers included: a project attached to one before it was
  // archived still has to say whose work it is. The engagement form's picker
  // asks the other way round.
  const { customers } = useCustomers(true);
  const [projects, setProjects] = useState<Project[]>([]);
  const [templates, setTemplates] = useState<ProjectTemplate[]>([]);
  const [editing, setEditing] = useState<Project | null>(null);
  const [editingDetails, setEditingDetails] = useState<Project | null>(null);
  const [creating, setCreating] = useState(false);
  const [startingFromTemplate, setStartingFromTemplate] = useState(false);
  const [invoiceHandoff, setInvoiceHandoff] = useState<{ project: Project; cutoff: string } | null>(null);
  const [isAdmin, setIsAdmin] = useState(false);
  const [loading, setLoading] = useState(true);
  const [projectsLoadFailed, setProjectsLoadFailed] = useState(false);
  const requestedContextProjectId = projectContextId(location.pathname, searchParams.get("project"));
  const contextProjectId = resolveProjectScope(requestedContextProjectId, loading, projects);
  const [revision, setRevision] = useState(0);
  const [runningTimer, setRunningTimer] = useState<RunningTimer | null>(null);
  const [error, setError] = useState<string | null>(null);

  const bump = useCallback(() => setRevision((r) => r + 1), []);

  const loadRunningTimer = useCallback(async () => {
    try {
      setRunningTimer(await api.timer());
    } catch {
      setRunningTimer(null);
    }
  }, [api]);

  useEffect(() => {
    void loadRunningTimer();
    return onTimerChanged(() => void loadRunningTimer());
  }, [loadRunningTimer]);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const list = await api.projects();
        if (live) {
          setProjects(list);
          setProjectsLoadFailed(false);
          setError(null);
        }
      } catch (err) {
        if (live) {
          setProjectsLoadFailed(true);
          setError(projectsMessage(err, strings.projectsLoadFailed));
        }
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, revision]);

  // A removed or inaccessible project is not a durable scope. Once the list is
  // authoritative, clean the query instead of showing "All projects" under a
  // URL that still claims otherwise and carrying that stale id to every tab.
  useEffect(() => {
    if (!searchParams.has("project")) return;
    if (!shouldRemoveProjectScope(
      requestedContextProjectId,
      loading,
      projectsLoadFailed,
      contextProjectId,
    )) return;
    const next = new URLSearchParams(searchParams);
    next.delete("project");
    setSearchParams(next, { replace: true });
  }, [contextProjectId, loading, projectsLoadFailed, requestedContextProjectId, searchParams, setSearchParams]);

  // The templates ride the same revision counter, because marking one, copying
  // one, or archiving a board all change what this list says.
  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const list = await api.templates();
        if (live) setTemplates(list);
      } catch (err) {
        if (live) setError(projectsMessage(err, strings.projectsTemplatesLoadFailed));
      }
    })();
    return () => {
      live = false;
    };
  }, [api, revision]);

  useEffect(() => {
    let live = true;
    void client
      .isAdmin()
      .then((ok) => {
        if (live) setIsAdmin(ok);
      })
      .catch(() => {
        // Not an admin, or the check is unavailable → the tab stays hidden,
        // which is the same thing a refusal would mean.
      });
    return () => {
      live = false;
    };
  }, [client]);

  /** A customer's own name for an id. `null` when the list has not loaded or
   *  the customer is not one this reader can see — the screen says "unknown"
   *  rather than printing a raw id at somebody. */
  const customerName = useCallback(
    (customerId: string) => customers.find((c) => c.id === customerId)?.name ?? null,
    [customers],
  );

  /** Starts the clock on a project. A timer already running is the server's
   *  `409` — shown as its own sentence, never turned into a silent stop of the
   *  one that is running. */
  async function startTimer(project: Project) {
    try {
      const timer = await api.startTimer({ projectId: project.id });
      setRunningTimer(timer);
      setError(null);
      announceTimerChanged();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsStartFailed));
    }
  }

  async function stopTimer() {
    try {
      await api.stopTimer();
      setRunningTimer(null);
      setError(null);
      announceTimerChanged();
      bump();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsStopFailed));
    }
  }

  async function createProject(draft: NewProjectDraft) {
    const project = await api.createProject(draft.name, draft.customerId);
    await api.updateProject(project.id, {
      name: draft.name,
      description: draft.description,
      status: draft.status,
      startsOn: draft.startsOn,
      targetOn: draft.targetOn,
    });
    setCreating(false);
    setError(null);
    navigate(`/projects/${encodeURIComponent(project.id)}/overview`);
  }

  async function updateProject(project: Project, draft: ProjectDraft) {
    await api.updateProject(project.id, draft);
    setEditingDetails(null);
    setError(null);
    bump();
  }

  useEffect(() => {
    if (searchParams.get("new") !== "1") return;
    setCreating(true);
    const next = new URLSearchParams(searchParams);
    next.delete("new");
    setSearchParams(next, { replace: true });
  }, [searchParams, setSearchParams]);

  /** Marks a board reusable, or takes the mark off. One control, because a board
   *  either is a template or is not — and undoing it is the same click again,
   *  which is why nothing here asks for a confirmation: the mark is a claim, not
   *  work, and taking it off destroys neither. */
  async function toggleTemplate(project: Project) {
    const marked = templates.some((t) => t.projectId === project.id);
    try {
      if (marked) await api.unmarkTemplate(project.id);
      else await api.markTemplate(project.id);
      setError(null);
      bump();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsTemplateFailed));
    }
  }

  return (
    <div className="relative flex h-full min-h-0 flex-col bg-app">
      <header className="shrink-0 border-b border-subtle bg-surface px-8 pt-6 max-sm:px-4 max-sm:pt-4">
        <div className="flex items-center gap-3">
          <h1 className="m-0 text-2xl font-bold text-primary">{strings.moduleProjects}</h1>
          {loading && <Spinner size={16} />}
        </div>
        <nav className="mt-3 flex min-w-0 gap-1 overflow-x-auto" aria-label={strings.moduleProjects}>
          <NavLink
            to="/projects/list"
            className={projectTabClass}
          >
            {strings.projectsTabList}
          </NavLink>
          <NavLink
            to="/projects/my-work"
            className={projectTabClass}
          >
            {strings.projectsTabMyWork}
          </NavLink>
          <NavLink
            to={projectScopedPath("week", contextProjectId)}
            className={projectTabClass}
          >
            {strings.projectsTabWeek}
          </NavLink>
          <NavLink
            to={projectScopedPath("timeline", contextProjectId)}
            className={projectTabClass}
          >
            {strings.projectsTabPlan}
          </NavLink>
          <NavLink
            to={projectScopedPath("reports", contextProjectId)}
            className={projectTabClass}
          >
            {strings.projectsTabReports}
          </NavLink>
          {isAdmin && (
            <NavLink
              to="/projects/approvals"
              className={projectTabClass}
            >
              {strings.projectsTabApprovals}
            </NavLink>
          )}
        </nav>
      </header>

      {error !== null && <ErrorBanner message={error} />}

      <Routes>
        <Route index element={<Navigate to="/projects/list" replace />} />
        <Route
          path="list"
          element={
            <ProjectsView
              projects={projects}
              loading={loading}
              runningTimer={runningTimer}
              customerName={customerName}
              isTemplate={(projectId) => templates.some((t) => t.projectId === projectId)}
              onEditClient={setEditing}
              onEditProject={setEditingDetails}
              onStartTimer={(project) => void startTimer(project)}
              onStopTimer={() => void stopTimer()}
              onToggleTemplate={(project) => void toggleTemplate(project)}
              onOpenTasks={(project) => navigate(`/projects/${encodeURIComponent(project.id)}/overview`)}
              onNewProject={() => setCreating(true)}
              onNewFromTemplate={() => setStartingFromTemplate(true)}
            />
          }
        />
        <Route
          path="my-work"
          element={<TasksModule projectsContext />}
        />
        <Route
          path="week"
          element={<WeekView projects={projects} projectsLoading={loading} revision={revision} onChanged={bump} />}
        />
        {/* The plan is a rendering of the board Tasks already shows — the same
            rows, grouped by the dates somebody planned them against — so it is
            everybody's tab too, and it names no person at all. */}
        <Route path="plan" element={<Navigate to="/projects/timeline" replace />} />
        <Route
          path="timeline"
          element={<PlanView projects={projects} projectsLoading={loading} revision={revision} onChanged={bump} />}
        />
        {/* Profitability is a PROJECT aggregate — engagements, minutes and
            money, and never who worked when — so it is everybody's tab, not
            the admin's (docs/design/projects.md § The hours of a person are
            personal data). */}
        <Route
          path="reports"
          element={(
            <ReportView
              projects={projects}
              projectsLoading={loading}
              customerName={customerName}
              revision={revision}
              onCreateInvoice={(project, cutoff) => setInvoiceHandoff({ project, cutoff })}
            />
          )}
        />
        {/* The admin tab is a route too, so a manager's bookmark works — and a
            non-admin who follows one gets the server's own `403` on the read
            rather than a page that pretends the inbox is empty. */}
        <Route
          path="approvals"
          element={(
            <ApprovalsView
              onDecided={bump}
              onOpenProject={(projectId) => navigate(`/projects/${encodeURIComponent(projectId)}/overview`)}
            />
          )}
        />
        <Route
          path=":projectId"
          element={(
            <ProjectWorkspaceRoute
              projects={projects}
              projectsLoading={loading}
              projectsLoadFailed={projectsLoadFailed}
            />
          )}
        />
        <Route
          path=":projectId/:workspaceView"
          element={(
            <ProjectWorkspaceRoute
              projects={projects}
              projectsLoading={loading}
              projectsLoadFailed={projectsLoadFailed}
            />
          )}
        />
        {/* An unknown Projects path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to="/projects/list" replace />} />
      </Routes>

      {startingFromTemplate && (
        <TemplateDialog
          templates={templates}
          defaultDay={today()}
          onClose={() => setStartingFromTemplate(false)}
          onCreated={(copy) => {
            setStartingFromTemplate(false);
            navigate(`/projects/${encodeURIComponent(copy.projectId)}/overview`);
          }}
        />
      )}

      {creating && (
        <NewProjectDialog customers={customers.filter((customer) => !customer.archived)} onClose={() => setCreating(false)} onCreate={createProject} />
      )}

      {editing !== null && (
        <ClientDialog
          project={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            bump();
          }}
        />
      )}

      {editingDetails !== null && (
        <EditProjectDialog
          project={editingDetails}
          onClose={() => setEditingDetails(null)}
          onSave={(draft) => updateProject(editingDetails, draft)}
        />
      )}

      {invoiceHandoff !== null && (
        <InvoiceHandoffDialog
          project={invoiceHandoff.project}
          initialCutoff={invoiceHandoff.cutoff}
          onClose={() => setInvoiceHandoff(null)}
          onCreated={(invoiceId) => {
            setInvoiceHandoff(null);
            navigate(`/billing/invoices/${encodeURIComponent(invoiceId)}`, {
              state: {
                fromProject: {
                  id: invoiceHandoff.project.id,
                  name: invoiceHandoff.project.name,
                },
              },
            });
          }}
        />
      )}
    </div>
  );
}
