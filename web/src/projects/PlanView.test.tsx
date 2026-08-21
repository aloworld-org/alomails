// @vitest-environment jsdom

import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import { PlanView } from "./PlanView";
import type { Project } from "./types";

const plan = vi.fn();
const tasks = vi.fn();
const projectsApi = { plan };
const jmapClient = { tasks };

vi.mock("./api", async (loadOriginal) => {
  const original = await loadOriginal<typeof import("./api")>();
  return {
    ...original,
    useProjectsApi: () => projectsApi,
  };
});

vi.mock("../jmap", async (loadOriginal) => {
  const original = await loadOriginal<typeof import("../jmap")>();
  return {
    ...original,
    useJmapClient: () => jmapClient,
  };
});

function project(id: string, name: string): Project {
  return {
    id,
    name,
    color: "#ed6b4f",
    kind: "internal",
    ownerId: "user-1",
    description: "",
    status: "active",
    startsOn: null,
    targetOn: null,
    createdAt: "2026-08-01T09:00:00Z",
    updatedAt: "2026-08-01T09:00:00Z",
    client: null,
    hours: {
      minutes: 0,
      billableMinutes: 0,
      approvedUnbilledMinutes: 0,
      submittedUnbilledMinutes: 0,
      billedMinutes: 0,
      budgetConsumptionBp: null,
      lastWorkedOn: null,
    },
    work: {
      openTasks: 0,
      overdueTasks: 0,
      blockedTasks: 0,
      nextDueAt: null,
    },
  };
}

const projects = [project("project-1", "Website"), project("project-2", "Launch")];

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("timeline project scope", () => {
  it("loads every project only when the portfolio scope is selected", async () => {
    plan.mockResolvedValue({ milestones: [], placements: [] });
    tasks.mockResolvedValue([]);

    render(
      <MemoryRouter initialEntries={["/projects/timeline"]}>
        <PlanView projects={projects} projectsLoading={false} revision={0} onChanged={vi.fn()} />
      </MemoryRouter>,
    );

    await waitFor(() => expect(plan).toHaveBeenCalledTimes(2));
    expect(plan).toHaveBeenCalledWith("project-1");
    expect(plan).toHaveBeenCalledWith("project-2");
    expect(tasks).toHaveBeenCalledWith("project-1");
    expect(tasks).toHaveBeenCalledWith("project-2");
  });

  it("loads only the project named in the URL", async () => {
    plan.mockResolvedValue({ milestones: [], placements: [] });
    tasks.mockResolvedValue([]);

    render(
      <MemoryRouter initialEntries={["/projects/timeline?project=project-2"]}>
        <PlanView projects={projects} projectsLoading={false} revision={0} onChanged={vi.fn()} />
      </MemoryRouter>,
    );

    await waitFor(() => expect(plan).toHaveBeenCalledTimes(1));
    expect(plan).toHaveBeenCalledWith("project-2");
    expect(tasks).toHaveBeenCalledTimes(1);
    expect(tasks).toHaveBeenCalledWith("project-2");
  });
});
