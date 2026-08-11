// The leave module's pure edges: the ones a screen makes awkward to reach and
// a wrong answer in makes somebody miss a week of work.
//
// Three of these are worth naming. A **grid that spans New Year** must ask for
// both years' holidays, or half a January calendar is silently unmarked. An
// absence that **starts today** is already begun, so the boundary is `>` and
// not `>=`. And the days somebody is shown are formatted **by the locale**,
// because half a day is written "12,5" in half of Europe.
import { describe, expect, test } from "vitest";

import { strings } from "../i18n";
import {
  absenceIndex,
  canCancel,
  canDecide,
  canWithdraw,
  daysLabel,
  holidayIndex,
  isWeekend,
  leaveStatusLabel,
  leaveStatusTone,
  monthOf,
  monthWeeks,
  peopleAway,
  shiftMonth,
  yearsOf,
} from "./leave";
import type { HrLeaveRequest, LeaveStatus } from "./types";

function request(over: Partial<HrLeaveRequest>): HrLeaveRequest {
  return {
    id: "req-1",
    employeeId: "emp-1",
    employeeName: "Ada Kowalski",
    policyId: "pol-1",
    policyName: "Annual leave",
    fromDay: "2026-09-01",
    toDay: "2026-09-04",
    status: "requested",
    note: "",
    costMinutes: 1920,
    workingDays: 4,
    holidayMinutes: 0,
    decidedBy: null,
    decidedAt: null,
    decisionNote: "",
    closedAt: null,
    createdAt: "2026-08-01T09:00:00Z",
    updatedAt: "2026-08-01T09:00:00Z",
    ...over,
  };
}

describe("days, as a person reads them", () => {
  test("a whole day loses its decimal and one day loses its number", () => {
    expect(daysLabel(10)).toBe(strings.hrOneDay);
    expect(daysLabel(250)).toBe(strings.hrDaysOf("25"));
    expect(daysLabel(0)).toBe(strings.hrDaysOf("0"));
  });

  test("a half day keeps it, and an overdrawn balance keeps its sign", () => {
    expect(daysLabel(125)).toBe(strings.hrDaysOf("12.5"));
    expect(daysLabel(-5)).toBe(strings.hrDaysOf("-0.5"));
  });
});

describe("what a row may offer", () => {
  test("only the person who asked may take a request back, and only while it waits", () => {
    expect(canWithdraw(request({}), "emp-1")).toBe(true);
    expect(canWithdraw(request({}), "emp-2")).toBe(false);
    expect(canWithdraw(request({}), null)).toBe(false);
    expect(canWithdraw(request({ status: "approved" }), "emp-1")).toBe(false);
  });

  test("booked leave can be given back until the day it starts, and not on it", () => {
    const booked = request({ status: "approved", fromDay: "2026-09-01" });
    expect(canCancel(booked, "2026-08-31")).toBe(true);
    // The first day of the absence is a day somebody was away: it is corrected
    // by HR with a reason, never erased by a button.
    expect(canCancel(booked, "2026-09-01")).toBe(false);
    expect(canCancel(booked, "2026-09-02")).toBe(false);
    expect(canCancel(request({ status: "requested" }), "2026-08-31")).toBe(false);
  });

  test("nobody is offered a decision on their own leave", () => {
    expect(canDecide(request({}), "emp-2")).toBe(true);
    expect(canDecide(request({}), "emp-1")).toBe(false);
    expect(canDecide(request({ status: "approved" }), "emp-2")).toBe(false);
  });
});

describe("the words for a state", () => {
  test("each of the five is named, and coloured by what it means", () => {
    const states: LeaveStatus[] = [
      "requested",
      "approved",
      "rejected",
      "withdrawn",
      "cancelled",
    ];
    for (const state of states) expect(leaveStatusLabel(state)).not.toBe(state);
    expect(leaveStatusTone("approved")).toBe("good");
    expect(leaveStatusTone("rejected")).toBe("bad");
    expect(leaveStatusTone("requested")).toBe("info");
  });

  test("a word from a newer server is shown rather than dropped", () => {
    expect(leaveStatusLabel("expired")).toBe("expired");
    expect(leaveStatusTone("expired")).toBe("info");
  });
});

describe("the month grid", () => {
  test("is always six Monday-first weeks around the month", () => {
    const weeks = monthWeeks("2026-08");
    expect(weeks).toHaveLength(6);
    expect(weeks.every((week) => week.length === 7)).toBe(true);
    // August 2026 starts on a Saturday, so the grid opens in July.
    expect(weeks[0]?.[0]).toBe("2026-07-27");
    expect(weeks[5]?.[6]).toBe("2026-09-06");
    expect(weeks.flat().filter((day) => day.startsWith("2026-08"))).toHaveLength(31);
  });

  test("a grid over New Year spans both years, and asks for both", () => {
    const weeks = monthWeeks("2026-01");
    expect(weeks[0]?.[0]).toBe("2025-12-29");
    expect(weeks[5]?.[6]).toBe("2026-02-08");
    expect(yearsOf(["2025-12-29", "2026-02-08"])).toEqual([2025, 2026]);
    expect(yearsOf(["2026-07-27", "2026-09-06"])).toEqual([2026]);
  });

  test("paging wraps the year in both directions", () => {
    expect(shiftMonth("2026-01", -1)).toBe("2025-12");
    expect(shiftMonth("2026-12", 1)).toBe("2027-01");
    expect(shiftMonth("2026-08", 1)).toBe("2026-09");
    expect(monthOf("2026-08-11")).toBe("2026-08");
  });

  test("Saturday and Sunday are the weekend and Friday is not", () => {
    expect(isWeekend("2026-08-08")).toBe(true);
    expect(isWeekend("2026-08-09")).toBe(true);
    expect(isWeekend("2026-08-07")).toBe(false);
  });
});

describe("the absence layer", () => {
  const days = [
    { day: "2026-08-10", people: [{ employeeId: "emp-2", name: "Bram" }] },
    {
      day: "2026-08-11",
      people: [
        { employeeId: "emp-2", name: "Bram" },
        { employeeId: "emp-3", name: "Chiara" },
      ],
    },
  ];

  test("is indexed by day, and days nobody is off are simply absent", () => {
    const index = absenceIndex(days);
    expect(index.get("2026-08-11")?.map((p) => p.name)).toEqual(["Bram", "Chiara"]);
    expect(index.get("2026-08-12")).toBeUndefined();
  });

  test("a person off across a window is named once, in the order they appear", () => {
    expect(peopleAway(days, null).map((p) => p.name)).toEqual(["Bram", "Chiara"]);
    expect(peopleAway(days, "emp-2").map((p) => p.name)).toEqual(["Chiara"]);
    expect(peopleAway([], null)).toEqual([]);
  });

  test("the tenant's holidays are indexed by the day they fall on", () => {
    const index = holidayIndex([
      { date: "2026-08-15", key: "assumption", name: "Assumption" },
    ]);
    expect(index.get("2026-08-15")).toBe("Assumption");
    expect(index.get("2026-08-16")).toBeUndefined();
  });
});
