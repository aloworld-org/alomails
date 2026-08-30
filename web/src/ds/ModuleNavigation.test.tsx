import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import {
  ModuleNavigation,
  moduleNavigationItemClassName,
} from "./ModuleNavigation";

afterEach(cleanup);

describe("ModuleNavigation", () => {
  test("protects sibling spacing and narrow-screen scrolling", () => {
    render(
      <ModuleNavigation label="Places">
        <a className={moduleNavigationItemClassName(true)} href="#current">
          Current
        </a>
        <a className={moduleNavigationItemClassName(false)} href="#next">
          Next
        </a>
      </ModuleNavigation>,
    );

    expect(screen.getByRole("navigation", { name: "Places" }).className).toBe(
      "flex min-w-0 gap-2 overflow-x-auto",
    );
    const currentClasses = new Set(
      screen.getByRole("link", { name: "Current" }).className.split(" "),
    );
    for (const className of [
      "min-h-11",
      "shrink-0",
      "items-center",
      "gap-2.5",
      "rounded-xl",
      "!px-4",
      "!py-2.5",
      "!text-sm",
      "[&_svg]:!size-4",
      "!font-semibold",
    ]) {
      expect(currentClasses.has(className)).toBe(true);
    }
    expect(screen.getByRole("link", { name: "Next" }).className).toContain(
      "!bg-transparent !font-medium !text-secondary",
    );
  });
});
