// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import { ReportView } from "./ReportView";
import type { ProfitabilityReport, Project } from "./types";

const profitability = vi.fn();
const profitabilityCsv = vi.fn();

vi.mock("./api", async (loadOriginal) => {
  const original = await loadOriginal<typeof import("./api")>();
  return {
    ...original,
    useProjectsApi: () => ({ profitability, profitabilityCsv }),
  };
});

const project: Project = {
  id: "project-1",
  name: "Website redesign",
  color: "#ed6b4f",
  kind: "client",
  ownerId: "user-1",
  description: "Refresh the public website.",
  status: "active",
  startsOn: "2026-08-01",
  targetOn: "2026-09-30",
  createdAt: "2026-08-01T09:00:00Z",
  updatedAt: "2026-08-01T09:00:00Z",
  client: {
    customerId: "customer-1",
    currency: "EUR",
    rateCents: 12_500,
    budgetMinutes: null,
    budgetCents: null,
    startsOn: "2026-08-01",
    createdAt: "2026-08-01T09:00:00Z",
    updatedAt: "2026-08-01T09:00:00Z",
  },
  hours: {
    minutes: 120,
    billableMinutes: 120,
    approvedUnbilledMinutes: 120,
    submittedUnbilledMinutes: 0,
    billedMinutes: 0,
    budgetConsumptionBp: null,
    lastWorkedOn: "2026-08-20",
  },
  work: {
    openTasks: 1,
    overdueTasks: 0,
    blockedTasks: 0,
    nextDueAt: null,
  },
};

const report: ProfitabilityReport = {
  from: "2026-07-01",
  to: "2026-09-30",
  projects: [
    {
      projectId: project.id,
      projectName: project.name,
      customerId: "customer-1",
      currency: "EUR",
      budgetMinutes: null,
      budgetCents: null,
      minutes: 120,
      billableMinutes: 120,
      unratedMinutes: 0,
      byCurrency: [
        {
          currency: "EUR",
          billableMinutes: 120,
          netCents: 25_000,
          billedMinutes: 0,
          billedNetCents: 0,
          unbilledNetCents: 25_000,
        },
      ],
      toDateMinutes: 120,
      toDateNetCents: 25_000,
      hoursConsumptionBp: null,
      budgetConsumptionBp: null,
      budgetRemainingCents: null,
    },
  ],
  totals: {
    minutes: 120,
    billableMinutes: 120,
    unratedMinutes: 0,
    byCurrency: [
      {
        currency: "EUR",
        billableMinutes: 120,
        netCents: 25_000,
        billedMinutes: 0,
        billedNetCents: 0,
        unbilledNetCents: 25_000,
      },
    ],
  },
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ReportView invoice handoff", () => {
  it("opens with the approved week and project carried through the URL", async () => {
    profitability.mockResolvedValue({
      ...report,
      from: "2026-08-17",
      to: "2026-08-23",
    });

    render(
      <MemoryRouter
        initialEntries={[
          "/projects/reports?project=project-1&from=2026-08-17&to=2026-08-23",
        ]}
      >
        <ReportView
          projects={[project]}
          projectsLoading={false}
          customerName={() => "Atelier Dupont SARL"}
          revision={0}
          onCreateInvoice={vi.fn()}
        />
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(profitability).toHaveBeenCalledWith(
        "2026-08-17",
        "2026-08-23",
        "project-1",
      );
    });
  });

  it("opens invoice creation for the reported project and cutoff", async () => {
    profitability.mockResolvedValue(report);
    const onCreateInvoice = vi.fn();

    render(
      <MemoryRouter>
        <ReportView
          projects={[project]}
          projectsLoading={false}
          customerName={() => "Atelier Dupont SARL"}
          revision={0}
          onCreateInvoice={onCreateInvoice}
        />
      </MemoryRouter>,
    );

    const action = await screen.findByRole("button", {
      name: "Create invoice: Website redesign",
    });
    fireEvent.click(action);

    expect(onCreateInvoice).toHaveBeenCalledWith(project, "2026-09-30");
  });

  it("keeps a zero unbilled amount informational", async () => {
    const reportedProject = report.projects[0];
    if (reportedProject === undefined)
      throw new Error("missing report fixture");
    const currency = reportedProject.byCurrency[0];
    if (currency === undefined) throw new Error("missing currency fixture");
    profitability.mockResolvedValue({
      ...report,
      projects: [
        {
          ...reportedProject,
          byCurrency: [{ ...currency, unbilledNetCents: 0 }],
        },
      ],
    });

    render(
      <MemoryRouter>
        <ReportView
          projects={[project]}
          projectsLoading={false}
          customerName={() => "Atelier Dupont SARL"}
          revision={0}
          onCreateInvoice={vi.fn()}
        />
      </MemoryRouter>,
    );

    await waitFor(() => expect(profitability).toHaveBeenCalled());
    expect(
      screen.queryByRole("button", {
        name: "Create invoice: Website redesign",
      }),
    ).toBeNull();
  });
});
