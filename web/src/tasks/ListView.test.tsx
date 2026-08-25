// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { Task } from "../jmap";
import { ListView } from "./ListView";
import { DEFAULT_CONFIG } from "./viewConfig";

function task(id: string, status: string, dueAt: string | null): Task {
  return {
    id,
    projectId: "project-1",
    title:
      id === "active"
        ? "Build responsive page templates"
        : "Confirm information architecture",
    description: null,
    status,
    position: id === "active" ? 1 : 2,
    assigneeId: "user-1",
    assignee: "designer@alo.test",
    dueAt,
    priority: id === "active" ? "high" : "none",
    state: "active",
    sourceKind: null,
    sourceId: null,
    subtaskDone: 0,
    subtaskTotal: 0,
    commentCount: 0,
    completedAt: status === "done" ? "2026-08-18T09:00:00Z" : null,
    createdAt: "2026-08-01T09:00:00Z",
  };
}

describe("premium task list", () => {
  it("summarises the visible workload and keeps task rows actionable", () => {
    const onOpen = vi.fn();
    render(
      <ListView
        tasks={[
          task("active", "in_progress", "2020-01-01T09:00:00Z"),
          task("done", "done", "2026-08-18T09:00:00Z"),
        ]}
        config={DEFAULT_CONFIG}
        projectName={() => "Northstar Website"}
        me="designer@alo.test"
        onOpen={onOpen}
        onMove={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    expect(screen.getByText("2 total")).toBeTruthy();
    expect(screen.getByText("1 active")).toBeTruthy();
    expect(screen.getByText("1 overdue")).toBeTruthy();
    expect(screen.getByText("1 completed")).toBeTruthy();

    fireEvent.click(screen.getByText("Build responsive page templates"));
    expect(onOpen).toHaveBeenCalledWith("active");
  });
});
