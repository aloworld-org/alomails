import { describe, expect, it } from "vitest";

import { upsertProject } from "./collection";
import type { Project } from "./types";

const project = (id: string, name: string) => ({ id, name }) as Project;

describe("upsertProject", () => {
  it("makes a newly created project immediately visible in the loaded portfolio", () => {
    const existing = project("project-1", "My tasks");
    const created = project("project-2", "my website");

    expect(upsertProject([existing], created)).toEqual([existing, created]);
  });

  it("replaces an existing project without duplicating it", () => {
    const updated = project("project-1", "Updated project");

    expect(upsertProject([project("project-1", "Old name")], updated)).toEqual([
      updated,
    ]);
  });
});
