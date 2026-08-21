import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import { strings } from "../i18n";
import { ApprovalsView } from "./ApprovalsView";

const approvals = vi.fn();
const approveWeek = vi.fn();
const rejectWeek = vi.fn();
const api = { approvals, approveWeek, rejectWeek };

vi.mock("./api", async (loadOriginal) => {
  const original = await loadOriginal<typeof import("./api")>();
  return { ...original, useProjectsApi: () => api };
});

vi.mock("../ds", async (loadOriginal) => {
  const original = await loadOriginal<typeof import("../ds")>();
  return { ...original, useDialogs: () => ({ prompt: vi.fn() }) };
});

const pendingWeek = {
  id: "week-1",
  weekStart: "2026-08-17",
  weekEnd: "2026-08-23",
  status: "submitted" as const,
  locked: true,
  submittedAt: "2026-08-23T17:00:00Z",
  decidedBy: null,
  decidedAt: null,
  decisionNote: "",
  createdAt: "2026-08-17T08:00:00Z",
  updatedAt: "2026-08-23T17:00:00Z",
  userId: "user-1",
  userEmail: "worker@example.com",
  minutes: 180,
  billableMinutes: 120,
  projects: [
    { projectId: "project-1", projectName: "Website", minutes: 120, billableMinutes: 120 },
    { projectId: "project-2", projectName: "Internal planning", minutes: 60, billableMinutes: 0 },
  ],
};

describe("ApprovalsView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    approvals.mockResolvedValue([pendingWeek]);
    approveWeek.mockResolvedValue({ ...pendingWeek, status: "approved" });
  });

  it("opens a project directly from a pending week", async () => {
    const onOpenProject = vi.fn();
    render(
      <MemoryRouter>
        <ApprovalsView onDecided={vi.fn()} onOpenProject={onOpenProject} />
      </MemoryRouter>,
    );

    fireEvent.click(await screen.findByRole("button", { name: "Website" }));
    expect(onOpenProject).toHaveBeenCalledWith("project-1");
  });

  it("keeps affected project links visible after approval", async () => {
    const onOpenProject = vi.fn();
    render(
      <MemoryRouter>
        <ApprovalsView onDecided={vi.fn()} onOpenProject={onOpenProject} />
      </MemoryRouter>,
    );

    fireEvent.click(await screen.findByRole("button", { name: strings.projectsApprove }));
    await waitFor(() => expect(approveWeek).toHaveBeenCalledWith("week-1"));
    const confirmation = await screen.findByRole("status");
    expect(within(confirmation).getByText(strings.projectsApprovalComplete)).toBeTruthy();

    expect(within(confirmation).getByRole("button", { name: /Website/ })).toBeTruthy();
    const reviewBilling = within(confirmation).getByRole("link", {
      name: strings.projectsReadyToInvoice,
    });
    expect(reviewBilling.getAttribute("href")).toBe("/projects/reports");
  });
});
