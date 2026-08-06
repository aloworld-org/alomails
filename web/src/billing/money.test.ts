// The parser is the only place in the web app where a human's typing becomes
// money, so every rule it claims is pinned here — including the ones that
// refuse, since a refusal is what stops a guessed price being stored.
import { describe, expect, it } from "vitest";

import {
  formatAmount,
  formatQty,
  formatRate,
  hundredthsToInput,
  milliToInput,
  parseHundredths,
  parseMilli,
} from "./money";

describe("parseHundredths", () => {
  it("reads a plain amount in either notation", () => {
    expect(parseHundredths("125")).toBe(12500);
    expect(parseHundredths("125.00")).toBe(12500);
    expect(parseHundredths("125,00")).toBe(12500);
    expect(parseHundredths("0.99")).toBe(99);
    expect(parseHundredths("0,9")).toBe(90);
    expect(parseHundredths(".5")).toBe(50);
  });

  it("never lets a float decide the cents", () => {
    // 1.15 * 100 is 114.999… in binary floating point; the price is 115.
    expect(parseHundredths("1.15")).toBe(115);
    expect(parseHundredths("8.29")).toBe(829);
    expect(parseHundredths("1234567.89")).toBe(123456789);
  });

  it("takes the last of two separators as the decimal one", () => {
    expect(parseHundredths("1.234,56")).toBe(123456);
    expect(parseHundredths("1,234.56")).toBe(123456);
    expect(parseHundredths("1.234.567,89")).toBe(123456789);
    expect(parseHundredths("1,234,567.89")).toBe(123456789);
  });

  it("treats a lone separator before three digits as grouping", () => {
    expect(parseHundredths("1.500")).toBe(150000);
    expect(parseHundredths("1,500")).toBe(150000);
    expect(parseHundredths("12.345.678")).toBe(1234567800);
  });

  it("drops the spaces a spreadsheet paste carries", () => {
    expect(parseHundredths(" 1 234,56 ")).toBe(123456);
    expect(parseHundredths("1 234.56")).toBe(123456);
    expect(parseHundredths("1 234,56")).toBe(123456);
  });

  it("keeps a sign, because the server owns the rule that refuses one", () => {
    // A negative unit price is refused by the store, not silently swallowed
    // here — the client must not hold a second definition of valid.
    expect(parseHundredths("-12.50")).toBe(-1250);
    expect(parseHundredths("+12.50")).toBe(1250);
  });

  it("refuses anything that is not a number", () => {
    for (const bad of ["", "   ", "abc", "12abc", "1.2.3,4,5", "€12", "12.345,678", "1.2345"]) {
      expect(parseHundredths(bad), bad).toBeNull();
    }
  });

  it("refuses a third decimal rather than rounding it away", () => {
    expect(parseHundredths("12,345")).toBe(1234500); // grouping — fifteen hundred style
    expect(parseHundredths("1.234,567")).toBeNull(); // an actual third decimal
  });

  it("refuses a value too large to stay an exact integer", () => {
    expect(parseHundredths("999999999999999999")).toBeNull();
  });

  it("round-trips through the editable form", () => {
    for (const cents of [0, 5, 50, 99, 100, 1250, 123456789, -1250]) {
      expect(parseHundredths(hundredthsToInput(cents))).toBe(cents);
    }
  });
});

describe("hundredthsToInput", () => {
  it("drops trailing zeros so a rate reads as a rate", () => {
    expect(hundredthsToInput(2100)).toBe("21");
    expect(hundredthsToInput(600)).toBe("6");
    expect(hundredthsToInput(0)).toBe("0");
    expect(hundredthsToInput(1250)).toBe("12.5");
    expect(hundredthsToInput(1205)).toBe("12.05");
    expect(hundredthsToInput(5)).toBe("0.05");
    expect(hundredthsToInput(-1250)).toBe("-12.5");
  });
});

describe("formatAmount", () => {
  it("always shows both decimals", () => {
    expect(formatAmount(12500, "en", "EUR")).toContain("125.00");
    expect(formatAmount(5, "en", "EUR")).toContain("0.05");
  });

  it("omits the symbol when no currency is given", () => {
    expect(formatAmount(12500, "en")).toBe("125.00");
  });

  it("falls back rather than blanking on an unknown currency", () => {
    expect(formatAmount(12500, "en", "ZZZ9")).toBe("125");
  });
});

describe("formatRate", () => {
  it("prints basis points as a percentage", () => {
    expect(formatRate(2100, "en")).toBe("21%");
    expect(formatRate(0, "en")).toBe("0%");
    expect(formatRate(550, "en")).toBe("5.5%");
  });
});

describe("parseMilli", () => {
  it("reads a quantity to three decimals in either notation", () => {
    expect(parseMilli("2")).toBe(2000);
    expect(parseMilli("1.5")).toBe(1500);
    expect(parseMilli("1,5")).toBe(1500);
    expect(parseMilli("0.125")).toBe(125);
    expect(parseMilli("0,25")).toBe(250);
    expect(parseMilli("-1")).toBe(-1000);
  });

  it("reads a separator as the decimal point, never as grouping", () => {
    // The one place the quantity rule differs from the money rule: "1.500"
    // pieces is one and a half, and a document must never bill a thousand
    // times what was typed because two conventions collided.
    expect(parseMilli("1.500")).toBe(1500);
    expect(parseMilli("1,500")).toBe(1500);
    expect(parseMilli("1500")).toBe(1500000);
    expect(parseMilli("1.234.567")).toBeNull();
  });

  it("refuses what it cannot read exactly", () => {
    expect(parseMilli("1.2345")).toBeNull();
    expect(parseMilli("two")).toBeNull();
    expect(parseMilli("")).toBeNull();
    expect(parseMilli("1..5")).toBeNull();
  });

  it("round-trips through the editable form", () => {
    for (const value of [0, 1000, 1500, 125, 1005, -2500]) {
      expect(parseMilli(milliToInput(value))).toBe(value);
    }
    expect(milliToInput(0)).toBe("0");
    expect(milliToInput(1500)).toBe("1.5");
    expect(milliToInput(125)).toBe("0.125");
    expect(milliToInput(1005)).toBe("1.005");
  });
});

describe("formatQty", () => {
  it("shows only the decimals a quantity has", () => {
    expect(formatQty(2000, "en")).toBe("2");
    expect(formatQty(1500, "en")).toBe("1.5");
    expect(formatQty(125, "en")).toBe("0.125");
    expect(formatQty(-1000, "en")).toBe("-1");
  });
});
