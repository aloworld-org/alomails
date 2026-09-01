import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { ProjectSalesOrigin } from "./ProjectSalesOrigin";

const salesOrigin = vi.fn();

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return {
    ...actual,
    useProjectsApi: () => ({ salesOrigin }),
  };
});

function Location() {
  return <output>{useLocation().pathname + useLocation().search}</output>;
}

describe("ProjectSalesOrigin", () => {
  beforeEach(() => {
    salesOrigin.mockReset();
  });

  test("opens the exact Sales opportunity that originated the project", async () => {
    salesOrigin.mockResolvedValue({
      dealId: "deal-1",
      dealTitle: "Premium rollout",
      projectId: "project-1",
      projectName: "Delivery",
      createdBy: "user-1",
      createdAt: "2026-09-01T10:00:00Z",
    });
    render(
      <MemoryRouter initialEntries={["/projects/project-1/overview"]}>
        <ProjectSalesOrigin projectId="project-1" />
        <Location />
      </MemoryRouter>,
    );

    expect(await screen.findByText("Premium rollout")).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: strings.projectsOpenSalesOrigin }),
    );
    expect(screen.getByText("/crm/board?deal=deal-1")).toBeTruthy();
  });

  test("renders nothing when the project has no Sales origin", async () => {
    salesOrigin.mockResolvedValue(null);
    const { container } = render(
      <MemoryRouter>
        <ProjectSalesOrigin projectId="project-1" />
      </MemoryRouter>,
    );
    await vi.waitFor(() => expect(salesOrigin).toHaveBeenCalledWith("project-1"));
    expect(container.querySelector("section")).toBeNull();
  });
});
