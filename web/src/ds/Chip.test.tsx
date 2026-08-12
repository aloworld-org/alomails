// The two things a chip can be, and the one it cannot.
//
// `Chip` grew a button form for mail's follow-up date (D2.02). The interesting
// property is not that it renders — it is that the two forms stay apart: a
// chip is either a button or a thing with a button in it, and a build has to
// say so, because nested buttons render perfectly happily and then swallow
// each other's clicks.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { Chip } from "./Chip";

afterEach(cleanup);

describe("Chip", () => {
  test("a chip with onClick is itself the button", () => {
    let pressed = 0;
    render(<Chip onClick={() => (pressed += 1)}>Due tomorrow</Chip>);
    const chip = screen.getByRole("button", { name: "Due tomorrow" });
    expect(chip.tagName).toBe("BUTTON");
    // A form's submit button by accident is how a chip in a compose window
    // sends the message; `type="button"` is the whole guard.
    expect(chip.getAttribute("type")).toBe("button");
    fireEvent.click(chip);
    expect(pressed).toBe(1);
  });

  test("a removable chip is not a button, but holds one that names its subject", () => {
    let removed = 0;
    render(
      <Chip onRemove={() => (removed += 1)} removeLabel="Remove Ada">
        Ada
      </Chip>,
    );
    const remove = screen.getByRole("button", { name: "Remove Ada" });
    expect(remove.closest("span")).not.toBeNull();
    fireEvent.click(remove);
    expect(removed).toBe(1);
    // Only the remove button — the chip around it is inert.
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  test("asking for both is reported, in development", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <Chip onClick={() => {}} onRemove={() => {}}>
        Both
      </Chip>,
    );
    expect(error.mock.calls.map(String).join(" ")).toContain(
      "onClick and onRemove",
    );
    error.mockRestore();
  });
});
