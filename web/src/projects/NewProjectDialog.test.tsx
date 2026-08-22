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

    await waitFor(() => expect(create).toHaveBeenCalledWith({ name: "Operations", customerId: null }));
  });

  test("requires and sends the customer for client work", async () => {
    const create = vi.fn().mockResolvedValue(undefined);
    render(<NewProjectDialog customers={[{ id: "customer-1", name: "Acme" }]} onClose={() => undefined} onCreate={create} />);

    fireEvent.change(screen.getByLabelText("Project name"), { target: { value: "Website" } });
    expect((screen.getByRole("button", { name: "Create project" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("combobox"));
    fireEvent.click(screen.getByRole("option", { name: "Acme" }));
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    await waitFor(() => expect(create).toHaveBeenCalledWith({ name: "Website", customerId: "customer-1" }));
  });
});
