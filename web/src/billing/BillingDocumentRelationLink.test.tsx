import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import { BillingDocumentRelationLink } from "./BillingDocumentRelationLink";

afterEach(cleanup);

test("opens the related billing document", () => {
  const onOpen = vi.fn();
  render(<BillingDocumentRelationLink label="Source quotation" onOpen={onOpen} />);
  fireEvent.click(screen.getByRole("button", { name: "Source quotation" }));
  expect(onOpen).toHaveBeenCalledOnce();
});
