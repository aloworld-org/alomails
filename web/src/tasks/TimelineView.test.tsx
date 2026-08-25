// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Task } from "../jmap";
import { TimelineView } from "./TimelineView";

afterEach(cleanup);

function task(id: string, title: string, dueAt: string | null): Task {
  return {
    id,
    projectId: "project-1",
    title,
    description: null,
    status: "in_progress",
    position: 1,
    assigneeId: "user-1",
    assignee: "designer@alo.test",
    dueAt,
    priority: "medium",
    state: "active",
    sourceKind: null,
    sourceId: null,
    subtaskDone: 0,
    subtaskTotal: 0,
    commentCount: 0,
    completedAt: null,
    createdAt: "2026-08-20T09:00:00Z",
  };
}

describe("project timeline", () => {
  it("opens scheduled and unscheduled tasks from the planning canvas", () => {
    const onOpen = vi.fn();
    render(
      <TimelineView
        tasks={[
          task("scheduled", "Build responsive templates", "2026-09-04T09:00:00Z"),
          task("unscheduled", "Plan launch retrospective", null),
        ]}
        onOpen={onOpen}
      />,
    );

    const scheduledBar = screen.getByRole("button", { name: "Build responsive templates: In progress" });
    fireEvent.click(scheduledBar);
    fireEvent.click(screen.getByRole("button", { name: "Plan launch retrospective" }));

    expect(onOpen).toHaveBeenNthCalledWith(1, "scheduled");
    expect(onOpen).toHaveBeenNthCalledWith(2, "unscheduled");
    expect(scheduledBar.textContent).toBe("");
  });
});
