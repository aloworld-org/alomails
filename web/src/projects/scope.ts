import type { Project } from "./types";

const TOP_LEVEL_PROJECT_ROUTES = new Set([
  "list",
  "my-work",
  "week",
  "plan",
  "timeline",
  "reports",
  "approvals",
]);

/** Keep the engagement visible while somebody moves through its work, time,
 * plan, and financial views. The path owns workspace scope; the three
 * aggregate views carry the same scope in their `project` query parameter. */
export function projectContextId(pathname: string, projectQuery: string | null): string | null {
  const parts = pathname.split("/").filter(Boolean);
  if (parts[0] !== "projects") return null;
  const segment = parts[1];
  if (segment !== undefined && !TOP_LEVEL_PROJECT_ROUTES.has(segment)) {
    try {
      return decodeURIComponent(segment);
    } catch {
      return segment;
    }
  }
  if (segment === "week" || segment === "timeline" || segment === "reports") {
    return projectQuery;
  }
  return null;
}

/** Builds the canonical route for a project-aware portfolio view. */
export function projectScopedPath(
  view: "week" | "timeline" | "reports",
  projectId: string | null,
): string {
  const path = `/projects/${view}`;
  return projectId === null
    ? path
    : `${path}?project=${encodeURIComponent(projectId)}`;
}

/** Resolves a URL scope only after the project collection is authoritative.
 * While it loads, retain the requested id so the screen never flashes the
 * portfolio view. Afterwards, an inaccessible or stale id becomes no scope. */
export function resolveProjectScope(
  requestedProjectId: string | null,
  projectsLoading: boolean,
  projects: Pick<Project, "id">[],
): string | null {
  if (requestedProjectId === null) return null;
  if (projectsLoading) return requestedProjectId;
  return projects.some((project) => project.id === requestedProjectId)
    ? requestedProjectId
    : null;
}

export type ProjectWorkspaceStatus = "loading" | "available" | "missing" | "unavailable";

/** Resolve a project deep link from the same authoritative collection that
 * drives the portfolio. A failed collection read is deliberately distinct
 * from a missing project: redirecting during an outage would turn a useful
 * bookmark into an apparently empty portfolio and hide the real problem. */
export function projectWorkspaceStatus(
  projectId: string,
  projectsLoading: boolean,
  projectsLoadFailed: boolean,
  projects: Pick<Project, "id">[],
): ProjectWorkspaceStatus {
  if (projectsLoading) return "loading";
  if (projectsLoadFailed) return "unavailable";
  return projects.some((project) => project.id === projectId)
    ? "available"
    : "missing";
}
