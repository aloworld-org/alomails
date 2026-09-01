import { describe, expect, test } from "vitest";

import { dealAttention, salesFocus } from "./salesFocus";
import type { CrmDeal } from "./types";

function deal(overrides: Partial<CrmDeal>): CrmDeal {
  return {
    id: "deal-1",
    pipelineId: "pipeline-1",
    stageId: "stage-1",
    title: "Renewal",
    customerId: null,
    contactId: null,
    companyName: "Northstar",
    contactName: "Ada",
    contactEmail: "ada@example.test",
    valueCents: 10_000,
    currency: "EUR",
    expectedClose: null,
    ownerUserId: "user-1",
    source: "Referral",
    position: 1,
    state: "open",
    closed: false,
    lostReason: null,
    closedAt: null,
    createdBy: "user-1",
    createdAt: "2026-08-01T09:00:00Z",
    updatedAt: "2026-08-28T09:00:00Z",
    ...overrides,
  };
}

describe("sales focus", () => {
  const now = new Date("2026-09-01T12:00:00Z");

  test("separates upcoming, overdue and quiet open deals", () => {
    const upcoming = deal({ id: "upcoming", expectedClose: "2026-09-10" });
    const overdue = deal({ id: "overdue", expectedClose: "2026-08-31" });
    const quiet = deal({ id: "quiet", updatedAt: "2026-08-01T09:00:00Z" });
    const won = deal({ id: "won", state: "won", closed: true, expectedClose: "2026-08-01" });

    const focus = salesFocus([upcoming, overdue, quiet, won], now);

    expect(focus.open).toHaveLength(3);
    expect(focus.closingSoon.map((item) => item.id)).toEqual(["upcoming"]);
    expect(focus.overdue.map((item) => item.id)).toEqual(["overdue"]);
    expect(focus.quiet.map((item) => item.id)).toEqual(["quiet"]);
    expect(focus.attention.map((item) => item.id)).toEqual(["overdue", "quiet"]);
  });

  test("gives an overdue close priority over a quiet update", () => {
    const overdueAndQuiet = deal({
      expectedClose: "2026-08-31",
      updatedAt: "2026-07-01T09:00:00Z",
    });

    expect(dealAttention(overdueAndQuiet, now)).toBe("overdue");
  });
});
