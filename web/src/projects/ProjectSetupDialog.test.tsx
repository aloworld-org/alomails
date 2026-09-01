import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { ProjectSetupDialog } from "./ProjectSetupDialog";

const setupProject = vi.fn();

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return { ...actual, useProjectsApi: () => ({ setupProject }) };
});

describe("ProjectSetupDialog", () => {
  beforeEach(() => setupProject.mockReset());

  test("creates nothing until the reviewed defaults are confirmed", async () => {
    const saved = vi.fn();
    setupProject.mockResolvedValue({ projectId: "project-1" });
    render(
      <ProjectSetupDialog
        projectId="project-1"
        projectName="Premium rollout"
        onClose={vi.fn()}
        onSaved={saved}
      />,
    );

    expect(setupProject).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: strings.projectsSetupConfirm }),
    );

    await vi.waitFor(() => expect(setupProject).toHaveBeenCalledTimes(1));
    expect(setupProject).toHaveBeenCalledWith("project-1", {
      createFilesSpace: true,
      createChatRoom: true,
      starterTasks: [
        strings.projectsSetupTaskScope,
        strings.projectsSetupTaskKickoff,
        strings.projectsSetupTaskPlan,
      ],
    });
    expect(saved).toHaveBeenCalledWith({ projectId: "project-1" });
  });
});
