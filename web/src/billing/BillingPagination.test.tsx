import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { BillingPagination } from "./BillingPagination";

test("navigates a deliberately paginated billing collection", () => {
  const onPage = vi.fn();
  render(<BillingPagination first={26} last={50} total={100} page={2} pageCount={4} onPage={onPage} />);
  expect(screen.getByText("26–50 of 100")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  expect(onPage).toHaveBeenCalledWith(3);
});
