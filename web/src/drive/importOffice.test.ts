import { describe, it, expect } from "vitest";
import { zipSync, strToU8 } from "fflate";

import { xlsxToUniverSnapshot } from "./importOffice";

/** Build a minimal but real `.xlsx` (OOXML zip) from XML parts. */
function makeXlsx(parts: Record<string, string>): Uint8Array {
  const entries: Record<string, Uint8Array> = {};
  for (const [path, xml] of Object.entries(parts)) entries[path] = strToU8(xml);
  return zipSync(entries);
}

const WORKBOOK = `<?xml version="1.0"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets><sheet name="Budget" sheetId="1" r:id="rId1"/></sheets>
</workbook>`;

const RELS = `<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://.../worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>`;

const SHARED = `<?xml version="1.0"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="2" uniqueCount="2">
  <si><t>Item</t></si>
  <si><t>Total</t></si>
</sst>`;

// A1="Item" (shared 0), B1=42 (number), A2="Total" (shared 1), C3=TRUE (bool).
const SHEET = `<?xml version="1.0"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" t="s"><v>0</v></c>
      <c r="B1"><v>42</v></c>
    </row>
    <row r="2"><c r="A2" t="s"><v>1</v></c></row>
    <row r="3"><c r="C3" t="b"><v>1</v></c></row>
  </sheetData>
</worksheet>`;

describe("xlsxToUniverSnapshot", () => {
  const snap = xlsxToUniverSnapshot(
    makeXlsx({
      "xl/workbook.xml": WORKBOOK,
      "xl/_rels/workbook.xml.rels": RELS,
      "xl/sharedStrings.xml": SHARED,
      "xl/worksheets/sheet1.xml": SHEET,
    }),
    "Budget",
  );
  const order = snap.sheetOrder as string[];
  const sheets = snap.sheets as Record<
    string,
    { name: string; cellData: Record<number, Record<number, { v: unknown; t: number }>> }
  >;
  const sheet = sheets[order[0] as string];

  it("keeps the workbook and sheet names", () => {
    expect(snap.name).toBe("Budget");
    expect(sheet?.name).toBe("Budget");
  });

  it("resolves shared strings by index", () => {
    expect(sheet?.cellData[0]?.[0]).toEqual({ v: "Item", t: 1 });
    expect(sheet?.cellData[1]?.[0]).toEqual({ v: "Total", t: 1 });
  });

  it("reads numbers as numeric cells", () => {
    expect(sheet?.cellData[0]?.[1]).toEqual({ v: 42, t: 2 });
  });

  it("reads booleans, mapping A1 refs to the right row/col", () => {
    // C3 → row index 2, col index 2.
    expect(sheet?.cellData[2]?.[2]).toEqual({ v: true, t: 3 });
  });

  it("does not throw on bytes that are not a zip", () => {
    expect(() => xlsxToUniverSnapshot(strToU8("not a zip"), "x")).toThrow();
  });
});
