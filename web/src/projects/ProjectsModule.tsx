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
import { NavLink, Navigate, Route, Routes } from "react-router-dom";

import { useCustomers } from "../billing";
import { Spinner } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { ApprovalsView } from "./ApprovalsView";
import { ClientDialog } from "./ClientDialog";
import { projectsMessage, useProjectsApi } from "./api";
import { ErrorBanner } from "./parts";
import { PlanView } from "./PlanView";
import { ProjectsView } from "./ProjectsView";
import { ReportView } from "./ReportView";
import { TemplateDialog } from "./TemplateDialog";
import { announceTimerChanged } from "./timerBus";
import { WeekView } from "./WeekView";
import type { Project, ProjectTemplate } from "./types";

/** Today as `YYYY-MM-DD` in the reader's own zone — the day a new project
 *  starts on unless they say otherwise. Local, not UTC: "today" is a fact about
 *  where the person is. */
function today(): string {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

export function ProjectsModule() {
  const api = useProjectsApi();
  const client = useJmapClient();
  // Archived customers included: a project attached to one before it was
  // archived still has to say whose work it is. The engagement form's picker
  // asks the other way round.
  const { customers } = useCustomers(true);
  const [projects, setProjects] = useState<Project[]>([]);
  const [templates, setTemplates] = useState<ProjectTemplate[]>([]);
  const [editing, setEditing] = useState<Project | null>(null);
  const [startingFromTemplate, setStartingFromTemplate] = useState(false);
  const [isAdmin, setIsAdmin] = useState(false);
  const [loading, setLoading] = useState(true);
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const bump = useCallback(() => setRevision((r) => r + 1), []);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const list = await api.projects();
        if (live) {
          setProjects(list);
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
  }, [api, revision]);

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
      await api.startTimer({ projectId: project.id });
      setError(null);
      announceTimerChanged();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsStartFailed));
    }
  }

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
      <header className="flex flex-wrap items-center gap-4 border-b border-subtle bg-surface px-5 py-3">
        <h1 className="m-0 text-xl font-semibold text-primary">{strings.moduleProjects}</h1>
        <nav className="ml-auto flex items-center gap-1 overflow-x-auto" aria-label={strings.moduleProjects}>
          <NavLink
            to="list"
            className={({ isActive }) =>
              `whitespace-nowrap rounded-lg px-3 py-2 text-sm font-medium no-underline transition-colors hover:no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${isActive ? "bg--soft text-accent" : "text-secondary hover:bg-raised hover:text-primary"}`
            }
          >
            {strings.projectsTabList}
          </NavLink>
          <NavLink
            to="week"
            className={({ isActive }) =>
              `whitespace-nowrap rounded-lg px-3 py-2 text-sm font-medium no-underline transition-colors hover:no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${isActive ? "bg--soft text-accent" : "text-secondary hover:bg-raised hover:text-primary"}`
            }
          >
            {strings.projectsTabWeek}
          </NavLink>
          <NavLink
            to="plan"
            className={({ isActive }) =>
              `whitespace-nowrap rounded-lg px-3 py-2 text-sm font-medium no-underline transition-colors hover:no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${isActive ? "bg--soft text-accent" : "text-secondary hover:bg-raised hover:text-primary"}`
            }
          >
            {strings.projectsTabPlan}
          </NavLink>
          <NavLink
            to="reports"
            className={({ isActive }) =>
              `whitespace-nowrap rounded-lg px-3 py-2 text-sm font-medium no-underline transition-colors hover:no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${isActive ? "bg--soft text-accent" : "text-secondary hover:bg-raised hover:text-primary"}`
            }
          >
            {strings.projectsTabReports}
          </NavLink>
          {isAdmin && (
            <NavLink
              to="approvals"
              className={({ isActive }) =>
                `whitespace-nowrap rounded-lg px-3 py-2 text-sm font-medium no-underline transition-colors hover:no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${isActive ? "bg--soft text-accent" : "text-secondary hover:bg-raised hover:text-primary"}`
              }
            >
              {strings.projectsTabApprovals}
            </NavLink>
          )}
        </nav>
        {loading && <Spinner size={16} />}
      </header>

      {error !== null && <ErrorBanner message={error} />}

      <Routes>
        <Route index element={<Navigate to="list" replace />} />
        <Route
          path="list"
          element={
            <ProjectsView
              projects={projects}
              loading={loading}
              customerName={customerName}
              isTemplate={(projectId) => templates.some((t) => t.projectId === projectId)}
              onEditClient={setEditing}
              onStartTimer={(project) => void startTimer(project)}
              onToggleTemplate={(project) => void toggleTemplate(project)}
              onNewFromTemplate={() => setStartingFromTemplate(true)}
            />
          }
        />
        <Route
          path="week"
          element={<WeekView projects={projects} revision={revision} onChanged={bump} />}
        />
        {/* The plan is a rendering of the board Tasks already shows — the same
            rows, grouped by the dates somebody planned them against — so it is
            everybody's tab too, and it names no person at all. */}
        <Route
          path="plan"
          element={<PlanView projects={projects} revision={revision} onChanged={bump} />}
        />
        {/* Profitability is a PROJECT aggregate — engagements, minutes and
            money, and never who worked when — so it is everybody's tab, not
            the admin's (docs/design/projects.md § The hours of a person are
            personal data). */}
        <Route
          path="reports"
          element={<ReportView customerName={customerName} revision={revision} />}
        />
        {/* The admin tab is a route too, so a manager's bookmark works — and a
            non-admin who follows one gets the server's own `403` on the read
            rather than a page that pretends the inbox is empty. */}
        <Route path="approvals" element={<ApprovalsView onDecided={bump} />} />
        {/* An unknown Projects path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to="list" replace />} />
      </Routes>

      {startingFromTemplate && (
        <TemplateDialog
          templates={templates}
          defaultDay={today()}
          onClose={() => setStartingFromTemplate(false)}
          onCreated={() => {
            setStartingFromTemplate(false);
            bump();
          }}
        />
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
    </div>
  );
}
