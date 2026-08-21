import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { strings } from "../i18n";
import { ProjectScopePicker } from "./ProjectScopePicker";

const projects = [
  { id: "project-1", name: "Website redesign" },
  { id: "project-2", name: "Autumn campaign" },
];

describe("ProjectScopePicker", () => {
  it("chooses a project from the shared scope menu", () => {
    const onChange = vi.fn();
    render(
      <ProjectScopePicker
        projects={projects}
        value={null}
        onChange={onChange}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", { name: strings.projectsAllProjects }),
    );
    expect(
      screen
        .getByRole("option", { name: strings.projectsAllProjects })
        .getAttribute("aria-selected"),
    ).toBe("true");

    fireEvent.click(screen.getByRole("option", { name: "Website redesign" }));
    expect(onChange).toHaveBeenCalledWith("project-1");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("shows the selected project and returns to the portfolio scope", () => {
    const onChange = vi.fn();
    render(
      <ProjectScopePicker
        projects={projects}
        value="project-2"
        onChange={onChange}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Autumn campaign" }));
    expect(
      screen
        .getByRole("option", { name: "Autumn campaign" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    fireEvent.click(
      screen.getByRole("option", { name: strings.projectsAllProjects }),
    );
    expect(onChange).toHaveBeenCalledWith(null);
  });
});
