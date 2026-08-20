import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { strings } from "../i18n";
import { EditProjectDialog } from "./EditProjectDialog";
import type { Project } from "./types";

const project: Project = {
  id: "project-1",
  name: "Website redesign",
  kind: "team",
  color: null,
  ownerId: "user-1",
  description: null,
  status: "planned",
  startsOn: "2026-08-20",
  targetOn: "2026-09-20",
  createdAt: "2026-08-20T08:00:00Z",
  updatedAt: "2026-08-20T08:00:00Z",
  client: null,
  hours: { minutes: 0, billableMinutes: 0, billedMinutes: 0, lastWorkedOn: null, budgetConsumptionBp: null },
};

describe("EditProjectDialog", () => {
  it("saves one coherent lifecycle record", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<EditProjectDialog project={project} onClose={vi.fn()} onSave={onSave} />);

    fireEvent.click(screen.getByRole("button", { name: strings.projectsStatusActive }));
    fireEvent.change(screen.getByLabelText(strings.projectsDescription), { target: { value: "Ship the new customer site." } });
    fireEvent.click(screen.getByRole("button", { name: strings.projectsSave }));

    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      name: "Website redesign",
      description: "Ship the new customer site.",
      status: "active",
      startsOn: "2026-08-20",
      targetOn: "2026-09-20",
    })));
  });
});
