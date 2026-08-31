import { cleanup, fireEvent, render, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { MessageSources } from "./MessageSources";

afterEach(cleanup);

describe("the footnotes behind an answer", () => {
  it("says nothing at all when the answer cited nothing", () => {
    const { container } = render(<MessageSources sources={[]} />);
    // Not an empty box, not a "0 sources" line: a message that cites nothing
    // — a person's line, an agent's plan — must look exactly as it did.
    expect(container.innerHTML).toBe("");
  });

  it("keeps the numbers the answer cites, and opens on asking", () => {
    const { container } = render(
      <MessageSources
        sources={[
          { n: 1, kind: "message", title: "Re: Harbor rollout" },
          { n: 2, kind: "remembered", title: "Ben owns the Harbor rollout" },
        ]}
      />,
    );
    const view = within(container);
    const toggle = view.getByRole("button");
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    // Closed, the answer is what is being read.
    expect(view.queryByText(/Ben owns the Harbor rollout/)).toBeNull();

    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    // [2] in the answer must find [2] here — the number is the whole point.
    expect(view.getByText("[2]")).toBeTruthy();
    expect(view.getByText(/Ben owns the Harbor rollout/)).toBeTruthy();
    // A kind that reads badly as a bare word is said the way a person says it.
    expect(view.getByText("email")).toBeTruthy();
  });

  it("shows a kind it has no word for exactly as it came", () => {
    // Reading tools are added whenever a module gains a verb, and their names
    // arrive here unannounced. Showing the name beats an empty space.
    const { container } = render(
      <MessageSources
        sources={[{ n: 1, kind: "open_quotes", title: "QUO-2026-00001" }]}
      />,
    );
    const view = within(container);
    fireEvent.click(view.getByRole("button"));
    expect(view.getByText("open_quotes")).toBeTruthy();
    expect(view.getByText(/QUO-2026-00001/)).toBeTruthy();
  });
});
