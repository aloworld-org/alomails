import { describe, expect, it } from "vitest";

import type { RunningTimer } from "../projects/types";
import { changeTaskTimer, taskTimerState } from "./TaskDetail";

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

describe("changeTaskTimer", () => {
  it("logs the current timer before switching to the selected task", async () => {
    const calls: string[] = [];
    const next = { ...timer, taskId: "task-2", note: "Review proposal" };
    const result = await changeTaskTimer(
      {
        stopTimer: async () => { calls.push("stop"); },
        startTimer: async (input) => {
          calls.push(`start:${input.taskId}`);
          return next;
        },
      },
      "another-task",
      { id: "task-2", projectId: "project-1", title: "Review proposal" },
    );

    expect(calls).toEqual(["stop", "start:task-2"]);
    expect(result).toEqual(next);
  });

  it("stops the selected task without starting another timer", async () => {
    const calls: string[] = [];
    const result = await changeTaskTimer(
      {
        stopTimer: async () => { calls.push("stop"); },
        startTimer: async () => {
          calls.push("start");
          return timer;
        },
      },
      "this-task",
      { id: "task-1", projectId: "project-1", title: "Prepare proposal" },
    );

    expect(calls).toEqual(["stop"]);
    expect(result).toBeNull();
  });
});
