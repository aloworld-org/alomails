// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Task } from "../jmap";
import { CalendarView } from "./CalendarView";

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date(2026, 7, 25, 9));
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function task(): Task {
  return {
    id: "task-1",
    projectId: "project-1",
    title: "Build responsive templates",
    description: null,
    status: "in_progress",
    position: 1,
    assigneeId: "user-1",
    assignee: "designer@alo.test",
    dueAt: "2026-08-28T09:00:00Z",
    priority: "high",
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

describe("project calendar", () => {
  it("opens a task without triggering the day create action", () => {
    const onOpen = vi.fn();
    const onAdd = vi.fn();
    render(<CalendarView tasks={[task()]} onOpen={onOpen} onAdd={onAdd} />);

    fireEvent.click(screen.getByRole("button", { name: "Build responsive templates" }));

    expect(onOpen).toHaveBeenCalledWith("task-1");
    expect(onAdd).not.toHaveBeenCalled();
    expect(screen.getByText("1 total")).toBeTruthy();
  });
});
