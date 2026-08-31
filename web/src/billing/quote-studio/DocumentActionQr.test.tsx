import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { DocumentActionQr } from "./DocumentActionQr";

test("labels the customer action encoded by the QR", () => {
  render(<DocumentActionQr value="mailto:test@example.com" label="Scan to accept" />);
  expect(screen.getAllByText("Scan to accept")).toHaveLength(2);
  expect(screen.getByTitle("Scan to accept")).toBeTruthy();
});
