// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { createElement } from "react";

import { showTimesheetHeaderAddTime, WeekView } from "./WeekView";
import type { Project } from "./types";

const time = vi.fn();
const weeks = vi.fn();
const projectsApi = { time, weeks };

vi.mock("./api", async (loadOriginal) => {
  const original = await loadOriginal<typeof import("./api")>();
  return {
    ...original,
    useProjectsApi: () => projectsApi,
  };
});

const project: Project = {
  id: "project-1",
  name: "Website redesign",
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
    minutes: 30,
    billableMinutes: 30,
    approvedUnbilledMinutes: 0,
    billedMinutes: 0,
    budgetConsumptionBp: null,
    lastWorkedOn: "2026-08-21",
  },
  work: {
    openTasks: 0,
    overdueTasks: 0,
    blockedTasks: 0,
    nextDueAt: null,
  },
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("timesheet primary action", () => {
  it("leaves the empty state's add action as the only primary action", () => {
    expect(showTimesheetHeaderAddTime(0, false, 2)).toBe(false);
  });

  it("shows the header action once the grid contains work", () => {
    expect(showTimesheetHeaderAddTime(1, false, 2)).toBe(true);
  });

  it("does not offer changes when the week is locked or no project exists", () => {
    expect(showTimesheetHeaderAddTime(1, true, 2)).toBe(false);
    expect(showTimesheetHeaderAddTime(1, false, 0)).toBe(false);
  });
});

describe("project-scoped timesheet submission", () => {
  it("shows the complete-week total beside the whole-week submit action", async () => {
    time.mockImplementation((_from: string, _to: string, projectId?: string) =>
      Promise.resolve({
        entries: [],
        totals:
          projectId === undefined
            ? { minutes: 120, billableMinutes: 90, proposedMinutes: 0 }
            : { minutes: 30, billableMinutes: 30, proposedMinutes: 0 },
      }),
    );
    weeks.mockResolvedValue([]);

    render(
      createElement(
        MemoryRouter,
        { initialEntries: ["/projects/week?project=project-1"] },
        createElement(WeekView, {
          projects: [project],
          projectsLoading: false,
          revision: 0,
          onChanged: vi.fn(),
        }),
      ),
    );

    await waitFor(() => expect(time).toHaveBeenCalledTimes(2));
    expect(time).toHaveBeenCalledWith(
      expect.any(String),
      expect.any(String),
      "project-1",
    );
    expect(time).toHaveBeenCalledWith(expect.any(String), expect.any(String));
    expect(screen.getByText("Entire week submitted for approval")).toBeTruthy();
    expect(screen.getByText("2h")).toBeTruthy();
    expect(screen.getByText("1h 30m billable")).toBeTruthy();
  });
});
