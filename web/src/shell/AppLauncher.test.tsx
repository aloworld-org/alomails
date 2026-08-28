import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { BriefcaseBusiness, MessageSquare, ReceiptText } from "lucide-react";
import { afterEach, describe, expect, test } from "vitest";
import { MemoryRouter } from "react-router-dom";

import { strings } from "../i18n";
import type { ProductModule } from "../product";
import { AppLauncher } from "./AppLauncher";

afterEach(cleanup);

const apps: ProductModule[] = [
  {
    id: "billing",
    path: "/billing",
    label: "Billing",
    Icon: ReceiptText,
    enabled: true,
  },
  {
    id: "projects",
    path: "/projects",
    label: "Projects",
    Icon: BriefcaseBusiness,
    enabled: true,
  },
  {
    id: "chat",
    path: "/chat",
    label: "Chat",
    Icon: MessageSquare,
    enabled: true,
  },
];

function renderLauncher() {
  return render(
    <MemoryRouter initialEntries={["/billing"]}>
      <AppLauncher apps={apps} favoriteModules={apps.slice(0, 2)} />
    </MemoryRouter>,
  );
}

describe("AppLauncher", () => {
  test("keeps the familiar nine-dot launcher mark", () => {
    renderLauncher();

    const trigger = screen.getByRole("button", { name: strings.appLauncher });
    expect(trigger.querySelectorAll("[data-launcher-dot]")).toHaveLength(9);
    expect(trigger.className).toContain("text-[#D7DEE2]");
    expect(trigger.className).toContain("hover:text-white");
  });

  test("shows favorites once and keeps the remaining catalogue quiet", () => {
    renderLauncher();

    fireEvent.click(screen.getByRole("button", { name: strings.appLauncher }));

    expect(screen.getAllByRole("link", { name: "Billing" })).toHaveLength(1);
    expect(screen.getAllByRole("link", { name: "Projects" })).toHaveLength(1);
    expect(screen.getAllByRole("link", { name: "Chat" })).toHaveLength(1);
    expect(screen.getByText(strings.appLauncherMore)).toBeTruthy();
  });

  test("closes after choosing an app and with Escape", () => {
    renderLauncher();

    const trigger = screen.getByRole("button", { name: strings.appLauncher });
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("link", { name: "Chat" }));
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.click(trigger);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
