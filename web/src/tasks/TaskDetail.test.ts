import { describe, expect, it } from "vitest";

import type { RunningTimer } from "../projects/types";
import { taskTimerState } from "./TaskDetail";

const timer: RunningTimer = {
  projectId: "project-1",
  taskId: "task-1",
  startedAt: "2026-08-20T09:00:00Z",
  billable: true,
  note: "Prepare proposal",
};

describe("taskTimerState", () => {
  it("offers a start action when no timer is running", () => {
    expect(taskTimerState(null, "task-1")).toBe("idle");
  });

  it("offers a stop action only for the task that owns the timer", () => {
    expect(taskTimerState(timer, "task-1")).toBe("this-task");
    expect(taskTimerState(timer, "task-2")).toBe("another-task");
  });

  it("does not treat a project-only timer as belonging to a task", () => {
    expect(taskTimerState({ ...timer, taskId: null }, "task-1")).toBe("another-task");
  });
});
