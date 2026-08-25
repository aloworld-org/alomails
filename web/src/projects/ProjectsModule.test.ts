import { describe, expect, it } from "vitest";

import {
  projectContextId,
  projectScopedPath,
  projectWorkspaceStatus,
  resolveProjectScope,
  shouldRemoveProjectScope,
} from "./scope";

describe("projectContextId", () => {
  it("keeps a project workspace visible across its nested task views", () => {
    expect(projectContextId("/projects/project%2Fone/overview", null)).toBe("project/one");
    expect(projectContextId("/projects/project-1/board", null)).toBe("project-1");
  });

  it("keeps project context in scoped time, plan, and report views", () => {
    expect(projectContextId("/projects/week", "project-1")).toBe("project-1");
    expect(projectContextId("/projects/timeline", "project-1")).toBe("project-1");
    expect(projectContextId("/projects/reports", "project-1")).toBeNull();
  });

  it("does not invent context for portfolio-level views", () => {
    expect(projectContextId("/projects/list", "project-1")).toBeNull();
    expect(projectContextId("/projects/my-work", null)).toBeNull();
    expect(projectContextId("/projects/approvals", "project-1")).toBeNull();
  });
});

describe("projectScopedPath", () => {
  it("carries project context between time, timeline, and report views", () => {
    expect(projectScopedPath("week", "project/one")).toBe(
      "/projects/week?project=project%2Fone",
    );
    expect(projectScopedPath("timeline", "project-1")).toBe(
      "/projects/timeline?project=project-1",
    );
    expect(projectScopedPath("reports", "project-1")).toBe("/projects/reports");
  });

  it("keeps portfolio routes free of an empty project query", () => {
    expect(projectScopedPath("week", null)).toBe("/projects/week");
  });
});

describe("resolveProjectScope", () => {
  const projects = [{ id: "project-1" }, { id: "project-2" }];

  it("retains deep-link scope while the authoritative project list loads", () => {
    expect(resolveProjectScope("project-1", true, [])).toBe("project-1");
  });

  it("accepts an accessible project after loading", () => {
    expect(resolveProjectScope("project-2", false, projects)).toBe("project-2");
  });

  it("rejects stale or inaccessible project ids after loading", () => {
    expect(resolveProjectScope("removed-project", false, projects)).toBeNull();
    expect(resolveProjectScope(null, false, projects)).toBeNull();
  });
});

describe("projectWorkspaceStatus", () => {
  const projects = [{ id: "project-1" }];

  it("does not treat a project as missing before the collection is authoritative", () => {
    expect(projectWorkspaceStatus("project-1", true, false, [])).toBe("loading");
  });

  it("distinguishes an unavailable collection from a missing project", () => {
    expect(projectWorkspaceStatus("project-1", false, true, [])).toBe("unavailable");
    expect(projectWorkspaceStatus("removed", false, false, projects)).toBe("missing");
  });

  it("opens an accessible project", () => {
    expect(projectWorkspaceStatus("project-1", false, false, projects)).toBe("available");
  });
});

describe("shouldRemoveProjectScope", () => {
  it("cleans a stale scope only after a successful authoritative read", () => {
    expect(shouldRemoveProjectScope("removed", false, false, null)).toBe(true);
    expect(shouldRemoveProjectScope("project-1", false, false, "project-1")).toBe(false);
  });

  it("preserves the chosen project during loading and collection failures", () => {
    expect(shouldRemoveProjectScope("project-1", true, false, null)).toBe(false);
    expect(shouldRemoveProjectScope("project-1", false, true, null)).toBe(false);
  });
});
