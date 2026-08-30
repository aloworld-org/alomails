import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { strings } from "../i18n";
import { ErrorBanner } from "./ErrorBanner";

afterEach(cleanup);

describe("ErrorBanner", () => {
  it("keeps contextual failures inline", () => {
    render(<ErrorBanner message="The invoice could not be saved." />);

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toBe("The invoice could not be saved.");
    expect(alert.className).not.toContain("fixed");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("presents list failures as a compact dismissible popup", () => {
    const onDismiss = vi.fn();
    render(
      <ErrorBanner
        message="Could not load this list."
        presentation="popup"
        onDismiss={onDismiss}
      />,
    );

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("Could not load this list.");
    expect(alert.parentElement?.className).toContain("fixed");

    fireEvent.click(screen.getByRole("button", { name: strings.close }));
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
