import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { TemplateDialog } from "./TemplateDialog";

vi.mock("../billing", () => ({
  useCustomers: () => ({ customers: [], error: null }),
}));

vi.mock("./api", () => ({
  projectsMessage: (_error: unknown, fallback: string) => fallback,
  useProjectsApi: () => ({ instantiateTemplate: vi.fn() }),
}));

describe("the project template dialog", () => {
  test("an empty catalogue offers a working route back instead of a disabled submit", () => {
    const close = vi.fn();

    render(
      <TemplateDialog
        templates={[]}
        defaultDay="2026-08-18"
        onClose={close}
        onCreated={() => undefined}
      />,
    );

    expect(screen.queryByRole("button", { name: "Create project" })).toBeNull();
    const choose = screen.getByRole("button", { name: "Choose a project" });
    expect((choose as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(choose);
    expect(close).toHaveBeenCalledOnce();
  });
});
