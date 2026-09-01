import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { ReportSchedulesView } from "./ReportSchedulesView";

const reportSchedules = vi.fn();
const createReportSchedule = vi.fn();
const deleteReportSchedule = vi.fn();
vi.mock("./api", () => ({
  useFinanceApi: () => ({
    reportSchedules,
    createReportSchedule,
    deleteReportSchedule,
  }),
  financeMessage: (_error: unknown, fallback: string) => fallback,
}));

beforeEach(() => {
  reportSchedules.mockReset().mockResolvedValue([]);
  createReportSchedule.mockReset().mockResolvedValue({});
  deleteReportSchedule.mockReset();
});

test("persists a complete scheduled report contract", async () => {
  render(
    <DialogProvider>
      <ReportSchedulesView />
    </DialogProvider>,
  );
  await screen.findByText(strings.financeNoSchedules);
  fireEvent.change(screen.getByLabelText(strings.financeReport), {
    target: { value: "aged_payable" },
  });
  fireEvent.change(screen.getByLabelText(strings.financeCadence), {
    target: { value: "quarterly" },
  });
  fireEvent.change(screen.getByLabelText(strings.financeRecipient), {
    target: { value: "finance@example.test" },
  });
  fireEvent.click(screen.getByLabelText(strings.financeNextDelivery));
  fireEvent.click(
    screen.getByRole("button", { name: strings.agendaNext }),
  );
  fireEvent.click(
    screen.getByRole("button", { name: /October 1, 2026/ }),
  );
  fireEvent.click(
    screen.getByRole("button", { name: strings.financeAddSchedule }),
  );
  await waitFor(() =>
    expect(createReportSchedule).toHaveBeenCalledWith({
      report: "aged_payable",
      cadence: "quarterly",
      recipient: "finance@example.test",
      nextRunDate: "2026-10-01",
    }),
  );
});
