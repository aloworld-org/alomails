import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SpendControlsView } from "./SpendControlsView";

const spendPolicy = vi.fn();
const saveSpendPolicy = vi.fn();
const api = { spendPolicy, saveSpendPolicy };
vi.mock("./api", () => ({
  useFinanceApi: () => api,
  financeMessage: (_error: unknown, fallback: string) => fallback,
}));

beforeEach(() => {
  spendPolicy.mockReset(); saveSpendPolicy.mockReset();
  spendPolicy.mockResolvedValue({ receiptRequiredAboveCents: null, projectRequiredAboveCents: null, secondApprovalAboveCents: null, currency: "EUR", updatedBy: null, updatedAt: null });
  saveSpendPolicy.mockImplementation(async (policy) => ({ ...policy, currency: "EUR", updatedBy: "u-1", updatedAt: "2026-09-01T12:00:00Z" }));
});

test("saves policy thresholds as integer cents through the Finance API", async () => {
  render(<SpendControlsView />);
  const toggles = await screen.findAllByRole("checkbox");
  fireEvent.click(toggles[0]!);
  fireEvent.change(screen.getByRole("textbox", { name: strings.financeReceiptRule }), { target: { value: "100.00" } });
  fireEvent.click(screen.getByRole("button", { name: strings.financeSavePolicy }));
  await waitFor(() => expect(saveSpendPolicy).toHaveBeenCalledWith({ receiptRequiredAboveCents: 10_000, projectRequiredAboveCents: null, secondApprovalAboveCents: null }));
  expect(await screen.findByText(strings.financePolicySaved)).toBeTruthy();
});
