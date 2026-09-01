import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MoreHorizontal } from "lucide-react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { Menu } from "./Menu";

afterEach(cleanup);

describe("Menu", () => {
  test("offers the shared comfortable size without changing hover rules", () => {
    render(
      <Menu
        label="Page actions"
        icon={<MoreHorizontal />}
        size="comfortable"
        items={[{ key: "rename", label: "Rename", onClick: vi.fn() }]}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Page actions" }));

    expect(screen.getByRole("menu").className).toContain("min-w-56 p-2.5");
    const itemClasses = screen.getByRole("menuitem", { name: "Rename" }).className;
    expect(itemClasses).toContain("min-h-11 !px-4 !py-2.5");
    expect(itemClasses).toContain("enabled:hover:!bg-accent-soft");
    expect(itemClasses).toContain("enabled:hover:!text-accent");
  });
});
