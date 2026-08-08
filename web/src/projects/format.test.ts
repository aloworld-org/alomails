// The edge where a person types a duration and the API wants whole minutes,
// and the edge where a stored day is turned back into one a person recognises.
//
// Both are where a timesheet goes wrong quietly: a duration read as 1.5 minutes
// instead of 90, or a day shown as the 29th because it was parsed as a UTC
// instant. Neither failure looks like a failure on screen, which is why they
// are tested here rather than trusted.
import { describe, expect, it } from "vitest";

import {
  MINUTES_PER_DAY,
  durationInput,
  durationLabel,
  mondayOf,
  parseDuration,
  shiftWeek,
  weekDays,
} from "./format";

describe("a typed duration", () => {
  it("reads the three spellings people actually type", () => {
    // Plain minutes.
    expect(parseDuration("90")).toBe(90);
    expect(parseDuration("45")).toBe(45);
    // Hours and minutes.
    expect(parseDuration("1:30")).toBe(90);
    expect(parseDuration("0:15")).toBe(15);
    expect(parseDuration("8:00")).toBe(480);
    // Decimal hours, in either European notation, with or without the h.
    expect(parseDuration("1,5")).toBe(90);
    expect(parseDuration("1.5")).toBe(90);
    expect(parseDuration("1,5h")).toBe(90);
    expect(parseDuration("0,25")).toBe(15);
  });

  it("reads the h as hours, which is the whole difference between 2 and 2h", () => {
    // A bare number on a timesheet is minutes; the letter is what says
    // otherwise. Stripping it and ignoring it would bill two minutes for two
    // hours, and look right on screen while doing it.
    expect(parseDuration("2")).toBe(2);
    expect(parseDuration(" 2 h ")).toBe(120);
    expect(parseDuration("8h")).toBe(480);
    // On the two spellings that already state hours it says nothing new.
    expect(parseDuration("1,5h")).toBe(90);
    expect(parseDuration("1:30h")).toBe(90);
  });

  it("rounds a decimal hour to the nearest minute rather than arguing", () => {
    // A tenth of an hour is six minutes exactly; a third is twenty. A form that
    // refused these would be arguing with the person who did the work.
    expect(parseDuration("0,1")).toBe(6);
    expect(parseDuration("0,333")).toBe(20);
    expect(parseDuration("1,01")).toBe(61);
  });

  it("refuses what is not a duration instead of storing a guess", () => {
    for (const bad of ["", "   ", "abc", "-30", "1:", "1:5m", "1:60x", "1,2,3", "12:345"]) {
      expect(parseDuration(bad), bad).toBeNull();
    }
  });

  it("refuses nothing and more than a day at both ends", () => {
    // Zero minutes is not work, and no day holds more than 24 hours — a night
    // shift over midnight is two entries, which is also how it must be billed.
    expect(parseDuration("0")).toBeNull();
    expect(parseDuration("0:00")).toBeNull();
    expect(parseDuration(String(MINUTES_PER_DAY))).toBe(MINUTES_PER_DAY);
    expect(parseDuration(String(MINUTES_PER_DAY + 1))).toBeNull();
    expect(parseDuration("25:00")).toBeNull();
  });

  it("survives the round trip through the field it is typed into", () => {
    for (const minutes of [1, 15, 60, 90, 435, MINUTES_PER_DAY]) {
      expect(parseDuration(durationInput(minutes))).toBe(minutes);
    }
    // Nothing is an empty field, never "0:00".
    expect(durationInput(0)).toBe("");
  });
});

describe("a duration for reading", () => {
  it("is written the way a person says one", () => {
    expect(durationLabel(90)).toBe("1h 30m");
    expect(durationLabel(480)).toBe("8h");
    expect(durationLabel(45)).toBe("45m");
  });

  it("shows nothing as a dash, because a blank cell reads as broken", () => {
    expect(durationLabel(0)).toBe("—");
  });
});

describe("the week a grid shows", () => {
  it("starts on Monday, whichever day it is asked about", () => {
    // A Wednesday, its own Monday, and the Sunday that ends the same week —
    // `getDay()` is 0 on Sunday, which is the off-by-six this guards.
    expect(mondayOf(new Date(2026, 7, 5))).toBe("2026-08-03");
    expect(mondayOf(new Date(2026, 7, 3))).toBe("2026-08-03");
    expect(mondayOf(new Date(2026, 7, 9))).toBe("2026-08-03");
    expect(mondayOf(new Date(2026, 7, 10))).toBe("2026-08-10");
  });

  it("is seven days, Monday to Sunday", () => {
    expect(weekDays("2026-08-03")).toEqual([
      "2026-08-03",
      "2026-08-04",
      "2026-08-05",
      "2026-08-06",
      "2026-08-07",
      "2026-08-08",
      "2026-08-09",
    ]);
  });

  it("steps across a month and a year without drifting", () => {
    expect(shiftWeek("2026-08-03", 1)).toBe("2026-08-10");
    expect(shiftWeek("2026-08-03", -1)).toBe("2026-07-27");
    expect(shiftWeek("2026-12-28", 1)).toBe("2027-01-04");
    // Across a spring-forward Sunday, where an instant-based +7×86 400 000 ms
    // would land an hour short and read as the previous day.
    expect(shiftWeek("2026-03-23", 1)).toBe("2026-03-30");
  });
});
