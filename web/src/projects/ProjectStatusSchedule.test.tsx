import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { ProjectStatusSchedule } from "./ProjectStatusSchedule";

describe("ProjectStatusSchedule", () => {
  test("changes status through the visible branded choices", () => {
    const onStatusChange = vi.fn();
    render(
      <ProjectStatusSchedule
        status="planned"
        startsOn="2026-08-01"
        targetOn="2026-08-31"
        datesValid
        onStatusChange={onStatusChange}
        onStartsOnChange={vi.fn()}
        onTargetOnChange={vi.fn()}
      />,
    );

    const active = screen.getByRole("button", { name: strings.projectsStatusActive });
    fireEvent.click(active);
    expect(onStatusChange).toHaveBeenCalledWith("active");
    expect(screen.getByRole("button", { name: strings.projectsStatusPlanned }).getAttribute("aria-pressed")).toBe("true");
  });

  test("shows the localized validation message for an invalid schedule", () => {
    render(
      <ProjectStatusSchedule
        status="active"
        startsOn="2026-09-01"
        targetOn="2026-08-31"
        datesValid={false}
        onStatusChange={vi.fn()}
        onStartsOnChange={vi.fn()}
        onTargetOnChange={vi.fn()}
      />,
    );

    expect(screen.getByText(strings.projectsDatesInvalid)).toBeTruthy();
  });
});
