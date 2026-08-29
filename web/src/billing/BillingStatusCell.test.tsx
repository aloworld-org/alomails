import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { strings } from "../i18n";
import { BillingStatusCell } from "./BillingStatusCell";

describe("BillingStatusCell", () => {
  test("keeps the separator on the table cell while laying out its badges", () => {
    render(
      <table>
        <tbody>
          <tr>
            <BillingStatusCell>
              <span>{strings.billingQuoteStatusSent}</span>
              <span>{strings.billingQuoteLapsed}</span>
            </BillingStatusCell>
          </tr>
        </tbody>
      </table>,
    );

    const cell = screen.getByRole("cell");
    expect(cell.className).not.toContain("inline-flex");
    expect(cell.firstElementChild?.className).toContain("inline-flex");
  });
});
