import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SiteSectionNavigation } from "./SiteSectionNavigation";

afterEach(cleanup);

describe("SiteSectionNavigation", () => {
  test("shows one selected workspace and changes it explicitly", () => {
    const onSelect = vi.fn();
    render(
      <SiteSectionNavigation
        active="pages"
        showCollaborators
        onSelect={onSelect}
      />,
    );

    expect(
      screen
        .getByRole("tab", { name: strings.sitesPages })
        .getAttribute("aria-selected"),
    ).toBe("true");
    expect(
      screen.getByRole("navigation", {
        name: strings.sitesWebsiteNavigation,
      }).className,
    ).toContain("gap-2 overflow-x-auto");
    expect(
      screen.getByRole("tab", { name: strings.sitesPages }).className,
    ).toContain("min-h-11");
    fireEvent.click(
      screen.getByRole("tab", { name: strings.sitesLanguages }),
    );
    expect(onSelect).toHaveBeenCalledWith("languages");
  });

  test("does not offer collaborator management without permission", () => {
    render(
      <SiteSectionNavigation
        active="pages"
        showCollaborators={false}
        onSelect={vi.fn()}
      />,
    );

    expect(
      screen.queryByRole("tab", { name: strings.sitesCollaborators }),
    ).toBeNull();
  });
});
