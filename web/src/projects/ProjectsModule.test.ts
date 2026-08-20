import { describe, expect, it } from "vitest";

import { projectContextId } from "./ProjectsModule";

describe("projectContextId", () => {
  it("keeps a project workspace visible across its nested task views", () => {
    expect(projectContextId("/projects/project%2Fone/overview", null)).toBe("project/one");
    expect(projectContextId("/projects/project-1/board", null)).toBe("project-1");
  });

  it("keeps project context in scoped time, plan, and report views", () => {
    expect(projectContextId("/projects/week", "project-1")).toBe("project-1");
    expect(projectContextId("/projects/timeline", "project-1")).toBe("project-1");
    expect(projectContextId("/projects/reports", "project-1")).toBe("project-1");
  });

  it("does not invent context for portfolio-level views", () => {
    expect(projectContextId("/projects/list", "project-1")).toBeNull();
    expect(projectContextId("/projects/my-work", null)).toBeNull();
    expect(projectContextId("/projects/approvals", "project-1")).toBeNull();
  });
});
