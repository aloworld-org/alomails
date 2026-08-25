// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Task } from "../jmap";
import { ListView } from "./ListView";
import { DEFAULT_CONFIG } from "./viewConfig";

afterEach(cleanup);

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
    const onMove = vi.fn();
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
        onMove={onMove}
        onAdd={vi.fn()}
        onConfigChange={vi.fn()}
      />,
    );

    expect(screen.getByText("2 total")).toBeTruthy();
    expect(screen.getByText("1 active")).toBeTruthy();
    expect(screen.getByText("1 overdue")).toBeTruthy();
    expect(screen.getByText("1 completed")).toBeTruthy();

    fireEvent.click(screen.getByText("Build responsive page templates"));
    expect(onOpen).toHaveBeenCalledWith("active");
    expect(onMove).not.toHaveBeenCalled();
  });

  it("moves a dragged task into the stage where it is dropped", () => {
    const onMove = vi.fn();
    render(
      <ListView
        tasks={[task("active", "in_progress", null)]}
        config={DEFAULT_CONFIG}
        projectName={() => "Northstar Website"}
        onOpen={vi.fn()}
        onMove={onMove}
        onConfigChange={vi.fn()}
      />,
    );

    const row = screen.getByText("Build responsive page templates").closest("[draggable]");
    const todoStage = screen.getByText("To do").closest("section");
    expect(row).not.toBeNull();
    expect(todoStage).not.toBeNull();

    const dataTransfer = {
      effectAllowed: "none",
      dropEffect: "none",
      setData: vi.fn(),
      getData: vi.fn(() => "active"),
    };
    fireEvent.dragStart(row as HTMLElement, { dataTransfer });
    fireEvent.dragOver(todoStage as HTMLElement, { dataTransfer });
    fireEvent.drop(todoStage as HTMLElement, { dataTransfer });

    expect(onMove).toHaveBeenCalledWith("active", "todo", 1024);
  });
});
