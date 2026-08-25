// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Task } from "../jmap";
import { FilesView } from "./FilesView";

const mocks = vi.hoisted(() => ({
  projectFiles: vi.fn(),
  tasks: vi.fn(),
}));

vi.mock("../jmap", async (importOriginal) => {
  const original = await importOriginal<typeof import("../jmap")>();
  return {
    ...original,
    useJmapClient: () => ({
      projectFiles: mocks.projectFiles,
      tasks: mocks.tasks,
    }),
  };
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const task: Task = {
  id: "task-1",
  projectId: "project-1",
  title: "Confirm information architecture",
  description: null,
  status: "in_progress",
  position: 1,
  assigneeId: null,
  assignee: null,
  dueAt: null,
  priority: "none",
  state: "active",
  sourceKind: null,
  sourceId: null,
  subtaskDone: 0,
  subtaskTotal: 0,
  commentCount: 0,
  completedAt: null,
  createdAt: "2026-08-20T09:00:00Z",
};

describe("project files", () => {
  it("presents attached files as cards that open their task", async () => {
    mocks.tasks.mockResolvedValue([task]);
    mocks.projectFiles.mockResolvedValue([{
      id: "file-1",
      blobId: "blob-1",
      filename: "wireframes.pdf",
      size: 2048,
      createdAt: "2026-08-25T09:00:00Z",
      taskId: task.id,
      taskTitle: task.title,
    }]);
    const onOpen = vi.fn();

    render(<FilesView projectId="project-1" onOpen={onOpen} onCreate={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "wireframes.pdf" }));

    expect(onOpen).toHaveBeenCalledWith("task-1");
    expect(screen.getByText("1 total")).toBeTruthy();
    expect(screen.getByText(/2 KB/)).toBeTruthy();
  });
});
