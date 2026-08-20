import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { strings } from "../i18n";
import { InvoiceHandoffDialog } from "./InvoiceHandoffDialog";
import type { Project } from "./types";

const unbilledTime = vi.fn();
const createTimeInvoice = vi.fn();
const api = { unbilledTime, createTimeInvoice };

vi.mock("./api", async (loadOriginal) => {
  const original = await loadOriginal<typeof import("./api")>();
  return {
    ...original,
    useProjectsApi: () => api,
  };
});

const project: Project = {
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
  client: {
    customerId: "customer-1",
    currency: "EUR",
    rateCents: 12000,
    budgetMinutes: null,
    budgetCents: null,
    startsOn: null,
    createdAt: "2026-08-20T08:00:00Z",
    updatedAt: "2026-08-20T08:00:00Z",
  },
  hours: { minutes: 120, billableMinutes: 120, approvedUnbilledMinutes: 120, billedMinutes: 0, lastWorkedOn: "2026-08-20", budgetConsumptionBp: null },
};

describe("InvoiceHandoffDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    unbilledTime.mockResolvedValue({
      customerId: "customer-1",
      groups: [{
        projectId: "project-1",
        projectName: "Website redesign",
        rateCents: 12000,
        currency: "EUR",
        minutes: 120,
        netCents: 24000,
        entryIds: ["entry-1", "entry-2"],
      }],
    });
    createTimeInvoice.mockResolvedValue({ id: "invoice-1", entries: 2, lines: 1, minutes: 120 });
  });

  it("loads approved time through the chosen cutoff and opens the created draft", async () => {
    const onCreated = vi.fn();
    render(<InvoiceHandoffDialog project={project} onClose={vi.fn()} onCreated={onCreated} />);

    const cutoff = screen.getByLabelText(strings.projectsInvoiceThrough);
    await waitFor(() => expect(unbilledTime).toHaveBeenCalledWith("customer-1", cutoff.getAttribute("value")));

    fireEvent.change(cutoff, { target: { value: "2026-08-19" } });
    await waitFor(() => expect(unbilledTime).toHaveBeenLastCalledWith("customer-1", "2026-08-19"));
    await screen.findByText("2h");
    fireEvent.click(screen.getByRole("button", { name: strings.projectsCreateDraftInvoice }));

    await waitFor(() => expect(createTimeInvoice).toHaveBeenCalledWith("customer-1", ["entry-1", "entry-2"]));
    expect(onCreated).toHaveBeenCalledWith("invoice-1");
  });
});
