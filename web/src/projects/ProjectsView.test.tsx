import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { strings } from "../i18n";
import { ProjectsView } from "./ProjectsView";
import type { Project, RunningTimer } from "./types";

const project: Project = {
  id: "project-1",
  name: "Website redesign",
  color: "#ed6b4f",
  kind: "team",
  ownerId: "user-1",
  description: "Refresh the public website.",
  status: "active",
  startsOn: "2026-08-01",
  targetOn: "2026-09-30",
  createdAt: "2026-08-01T09:00:00Z",
  updatedAt: "2026-08-01T09:00:00Z",
  client: null,
  hours: {
    minutes: 0,
    billableMinutes: 0,
    billedMinutes: 0,
    budgetConsumptionBp: null,
    lastWorkedOn: null,
  },
};

const timer: RunningTimer = {
  projectId: project.id,
  taskId: null,
  startedAt: new Date().toISOString(),
  billable: false,
  note: "",
};

describe("ProjectsView running timer", () => {
  it("shows the active timer in the main workspace and replaces the row start action", () => {
    render(
      <ProjectsView
        projects={[project]}
        loading={false}
        runningTimer={timer}
        customerName={() => null}
        isTemplate={() => false}
        onEditClient={vi.fn()}
        onEditProject={vi.fn()}
        onStartTimer={vi.fn()}
        onStopTimer={vi.fn()}
        onToggleTemplate={vi.fn()}
        onOpenTasks={vi.fn()}
        onNewProject={vi.fn()}
        onNewFromTemplate={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: strings.projectsStopTimer })).toBeTruthy();
    expect(screen.getByText(strings.projectsTimerRunning)).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: strings.projectsStartTimerOn(project.name) }),
    ).toBeNull();
  });
});
