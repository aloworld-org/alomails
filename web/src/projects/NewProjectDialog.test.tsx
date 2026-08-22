import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { NewProjectDialog } from "./NewProjectDialog";

afterEach(cleanup);

describe("the new project journey", () => {
  test("creates internal work without making the user choose a customer", async () => {
    const create = vi.fn().mockResolvedValue(undefined);
    render(<NewProjectDialog customers={[]} onClose={() => undefined} onCreate={create} />);

    fireEvent.change(screen.getByLabelText("Project name"), { target: { value: "Operations" } });
    expect(screen.getByRole("radio", { name: /Our company/ }).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    await waitFor(() => expect(create).toHaveBeenCalledWith({
      name: "Operations",
      customerId: null,
      description: null,
      status: "planned",
      startsOn: null,
      targetOn: null,
    }));
  });

  test("requires and sends the customer for client work", async () => {
    const create = vi.fn().mockResolvedValue(undefined);
    render(<NewProjectDialog customers={[{ id: "customer-1", name: "Acme" }]} onClose={() => undefined} onCreate={create} />);

    fireEvent.change(screen.getByLabelText("Project name"), { target: { value: "Website" } });
    expect((screen.getByRole("button", { name: "Create project" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("combobox"));
    fireEvent.click(screen.getByRole("option", { name: "Acme" }));
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    await waitFor(() => expect(create).toHaveBeenCalledWith({
      name: "Website",
      customerId: "customer-1",
      description: null,
      status: "planned",
      startsOn: null,
      targetOn: null,
    }));
  });

  test("creates a fully configured project in one journey", async () => {
    const create = vi.fn().mockResolvedValue(undefined);
    render(<NewProjectDialog customers={[]} onClose={() => undefined} onCreate={create} />);

    fireEvent.change(screen.getByLabelText("Project name"), { target: { value: "Website" } });
    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "Launch the new site." } });
    fireEvent.click(screen.getByRole("button", { name: "Active" }));
    fireEvent.change(screen.getByLabelText("Starts on"), { target: { value: "2026-08-24" } });
    fireEvent.change(screen.getByLabelText("Target date"), { target: { value: "2026-09-30" } });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    await waitFor(() => expect(create).toHaveBeenCalledWith({
      name: "Website",
      customerId: null,
      description: "Launch the new site.",
      status: "active",
      startsOn: "2026-08-24",
      targetOn: "2026-09-30",
    }));
  });

  test("explains when client work has no available customers", () => {
    render(<NewProjectDialog customers={[]} onClose={() => undefined} onCreate={vi.fn()} />);

    fireEvent.click(screen.getByRole("radio", { name: /A customer/ }));
    fireEvent.click(screen.getByRole("combobox"));

    expect(screen.getByText("No customers are available yet. Add one in Billing first.")).toBeTruthy();
  });
});
