import { describe, expect, it } from "vitest";

import { showTimesheetHeaderAddTime } from "./WeekView";

describe("timesheet primary action", () => {
  it("leaves the empty state's add action as the only primary action", () => {
    expect(showTimesheetHeaderAddTime(0, false, 2)).toBe(false);
  });

  it("shows the header action once the grid contains work", () => {
    expect(showTimesheetHeaderAddTime(1, false, 2)).toBe(true);
  });

  it("does not offer changes when the week is locked or no project exists", () => {
    expect(showTimesheetHeaderAddTime(1, true, 2)).toBe(false);
    expect(showTimesheetHeaderAddTime(1, false, 0)).toBe(false);
  });
});
