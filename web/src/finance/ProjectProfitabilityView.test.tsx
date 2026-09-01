import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { ProjectProfitabilityView } from "./ProjectProfitabilityView";

const profitability = vi.fn();
vi.mock("../projects/api", () => ({
  useProjectsApi: () => ({ profitability }),
  projectsMessage: (_error: unknown, fallback: string) => fallback,
}));

beforeEach(() => {
  profitability.mockReset();
  profitability.mockResolvedValue({
    from: "2026-07-01", to: "2026-09-30",
    totals: { minutes: 600, billableMinutes: 540, unratedMinutes: 60, byCurrency: [{ currency: "EUR", billableMinutes: 480, netCents: 80_000, billedMinutes: 180, billedNetCents: 30_000, unbilledNetCents: 50_000 }] },
    projects: [{ projectId: "p-1", projectName: "Website rollout", customerId: "c-1", currency: "EUR", budgetMinutes: 600, budgetCents: 70_000, minutes: 600, billableMinutes: 540, unratedMinutes: 60, byCurrency: [{ currency: "EUR", billableMinutes: 480, netCents: 80_000, billedMinutes: 180, billedNetCents: 30_000, unbilledNetCents: 50_000 }], toDateMinutes: 660, toDateNetCents: 80_000, hoursConsumptionBp: 11_000, budgetConsumptionBp: 11_429, budgetRemainingCents: -10_000 }],
  });
});

test("surfaces server-owned project budget and unbilled exceptions in Finance", async () => {
  render(<MemoryRouter><ProjectProfitabilityView /></MemoryRouter>);
  await waitFor(() => expect(profitability).toHaveBeenCalled());
  expect(await screen.findByText("Website rollout")).toBeTruthy();
  expect(screen.getByText(strings.financeOverBudget)).toBeTruthy();
  expect(screen.getByText(strings.financeUnratedMinutes(60))).toBeTruthy();
  expect(screen.getAllByText(/500/).length).toBeGreaterThan(0);
});
