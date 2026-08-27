import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ListBlockEditor } from "./ListBlockEditor";

describe("ListBlockEditor", () => {
  it("renders each list item", () => {
    render(<ListBlockEditor ordered={false} items={"One\nTwo"} columns={1} onChange={vi.fn()} />);
    expect(screen.getByText("One")).not.toBeNull();
    expect(screen.getByText("Two")).not.toBeNull();
  });
});
