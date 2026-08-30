import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import type { BillingCustomer, BillingScheduleSummary } from "./types";

const { billingApi } = vi.hoisted(() => ({
  billingApi: {
    schedules: vi.fn(),
    customers: vi.fn(),
    runSchedules: vi.fn(),
    setScheduleActive: vi.fn(),
    deleteSchedule: vi.fn(),
    updateSchedule: vi.fn(),
  },
}));

vi.mock("./api", async (loadOriginal) => {
  const original = await loadOriginal<typeof import("./api")>();
  return { ...original, useBillingApi: () => billingApi };
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: vi.fn() }),
}));

import { SchedulesView } from "./SchedulesView";

const customer = {
  id: "customer-2",
  name: "Juniper Studio 2",
} as BillingCustomer;

const schedule: BillingScheduleSummary = {
  id: "schedule-2",
  customerId: customer.id,
  name: "Juniper recurring plan 002",
  cadence: "monthly",
  anchorDay: 2,
  startDate: "2026-08-02",
  endDate: null,
  nextRunDate: "2026-09-02",
  lastRunDate: "2026-08-02",
  active: true,
  ended: false,
  due: false,
  currency: "EUR",
  paymentTermsDays: 30,
  reference: "",
  note: "",
  createdBy: "user-1",
  createdAt: "2026-08-01T10:00:00Z",
  updatedAt: "2026-08-29T10:00:00Z",
  totals: {
    netCents: 4381,
    vatCents: 920,
    grossCents: 5301,
    vatByRate: [{ rateBp: 2100, netCents: 4381, vatCents: 920 }],
  },
  raisedCount: 1,
};

function renderView() {
  billingApi.schedules.mockResolvedValue([schedule]);
  billingApi.customers.mockResolvedValue([customer]);
  billingApi.setScheduleActive.mockResolvedValue(undefined);
  return render(
    <MemoryRouter>
      <DialogProvider>
        <SchedulesView />
      </DialogProvider>
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("SchedulesView", () => {
  test("opens the exact recurring schedule from the entire row with pointer or keyboard", async () => {
    renderView();

    const row = await screen.findByRole("button", {
      name: `${schedule.name}: ${customer.name}`,
    });
    fireEvent.click(row);
    expect(await screen.findByRole("dialog", { name: strings.billingRecurringTitle })).toBeTruthy();

    fireEvent.click(screen.getAllByRole("button", { name: strings.billingCancel }).at(-1)!);
    fireEvent.keyDown(row, { key: "Enter" });
    expect(await screen.findByRole("dialog", { name: strings.billingRecurringTitle })).toBeTruthy();
  });

  test("keeps the pause action independent from the row open action", async () => {
    renderView();

    fireEvent.click(await screen.findByRole("button", { name: strings.billingSchedulePause }));

    await waitFor(() => expect(billingApi.setScheduleActive).toHaveBeenCalledWith(schedule.id, false));
    expect(screen.queryByRole("dialog", { name: strings.billingRecurringTitle })).toBeNull();
  });
});
