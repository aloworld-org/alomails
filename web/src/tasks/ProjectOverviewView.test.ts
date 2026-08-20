import { describe, expect, it } from "vitest";

import type { Project } from "../projects/types";
import { canCreateProjectInvoice, projectNextStep } from "./ProjectOverviewView";

function project(approvedUnbilledMinutes: number, client = true): Project {
  return {
    id: "project-1",
    name: "Website redesign",
    kind: "team",
    color: null,
    ownerId: "user-1",
    description: null,
    status: "active",
    startsOn: null,
    targetOn: null,
    createdAt: "2026-08-20T08:00:00Z",
    updatedAt: "2026-08-20T08:00:00Z",
    client: client ? {
      customerId: "customer-1",
      currency: "EUR",
      rateCents: 12_000,
      budgetMinutes: null,
      budgetCents: null,
      startsOn: null,
      createdAt: "2026-08-20T08:00:00Z",
      updatedAt: "2026-08-20T08:00:00Z",
    } : null,
    hours: {
      minutes: 180,
      billableMinutes: 180,
      approvedUnbilledMinutes,
      billedMinutes: 0,
      lastWorkedOn: "2026-08-20",
      budgetConsumptionBp: null,
    },
    work: { openTasks: 0, overdueTasks: 0, blockedTasks: 0, nextDueAt: null },
  };
}

describe("project invoice readiness", () => {
  it("does not offer invoicing for billable time until it is approved", () => {
    expect(canCreateProjectInvoice(project(0))).toBe(false);
  });

  it("offers invoicing for approved unbilled client time only", () => {
    expect(canCreateProjectInvoice(project(120))).toBe(true);
    expect(canCreateProjectInvoice(project(120, false))).toBe(false);
  });
});

describe("project workflow next step", () => {
  it("guides a new project from tasks to time", () => {
    const value = project(0);
    value.hours.minutes = 0;
    value.hours.billableMinutes = 0;
    expect(projectNextStep(value, 0)).toBe("tasks");
    expect(projectNextStep(value, 1)).toBe("time");
  });

  it("guides unapproved billable time to review and approved time to invoicing", () => {
    const value = project(0);
    expect(projectNextStep(value, 1)).toBe("approval");
    value.hours.approvedUnbilledMinutes = 120;
    expect(projectNextStep(value, 1)).toBe("invoice");
  });

  it("never offers the invoice workflow for internal work", () => {
    expect(projectNextStep(project(120, false), 1)).toBe("continue");
  });
});
