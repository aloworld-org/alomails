import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ReceiptText } from "lucide-react";
import { afterEach, describe, expect, test, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";

import type { ProductModule } from "../product";
import { AppTile } from "./AppTile";

afterEach(cleanup);

const app: ProductModule = {
  id: "billing",
  path: "/billing",
  label: "Billing",
  Icon: ReceiptText,
  enabled: true,
};

describe("AppTile", () => {
  test("uses the registered route and exposes the current app", () => {
    render(
      <MemoryRouter initialEntries={["/billing"]}>
        <AppTile app={app} onSelect={() => undefined} />
      </MemoryRouter>,
    );

    const link = screen.getByRole("link", { name: app.label });
    expect(link.getAttribute("href")).toBe(app.path);
    expect(link.getAttribute("aria-current")).toBe("page");
  });

  test("reports selection without changing the registered destination", () => {
    const onSelect = vi.fn();
    render(
      <MemoryRouter>
        <AppTile app={app} onSelect={onSelect} />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("link", { name: app.label }));
    expect(onSelect).toHaveBeenCalledOnce();
  });
});
