// The directory's thinking, tested where it lives: the search, the manager
// lookup and the one thing a tree cannot borrow from a list — narrowing without
// lying about who works for whom. The screen's own promises are in
// `Directory.test.tsx`; these are the edges a screen makes awkward to reach.
import { describe, expect, test } from "vitest";

import { byId, countOrg, filterDirectory, filterOrg, managerName, searchTerms } from "./directory";
import type { HrDirectoryEntry, HrOrgNode } from "./types";

function person(over: Partial<HrDirectoryEntry> & { id: string; name: string }): HrDirectoryEntry {
  return {
    givenName: "",
    familyName: "",
    preferredName: "",
    workEmail: "",
    workPhone: "",
    managerId: null,
    photoNodeId: null,
    jobTitle: "",
    team: "",
    startedOn: null,
    archived: false,
    ...over,
  };
}

function node(
  id: string,
  name: string,
  reports: HrOrgNode[] = [],
  jobTitle = "",
  team = "",
): HrOrgNode {
  return { id, name, jobTitle, team, managerId: null, reports };
}

const PEOPLE = [
  person({ id: "1", name: "Ada Kowalski", familyName: "Kowalski", team: "Board" }),
  person({
    id: "2",
    name: "Bram de Vries",
    familyName: "de Vries",
    team: "Platform",
    jobTitle: "Engineer",
    managerId: "1",
    workEmail: "bram@example.test",
  }),
  // Managed by somebody who is not in this answer — an archived colleague while
  // the list holds only the people who are here.
  person({ id: "3", name: "Chiara Rossi", managerId: "99" }),
];

describe("searching the people", () => {
  test("an empty box is not a filter", () => {
    expect(searchTerms("   ")).toEqual([]);
    expect(filterDirectory(PEOPLE, "  ")).toBe(PEOPLE);
  });

  test("every word must match, in any field and in any order", () => {
    expect(filterDirectory(PEOPLE, "platform vries").map((p) => p.id)).toEqual(["2"]);
    expect(filterDirectory(PEOPLE, "VRIES PLATFORM").map((p) => p.id)).toEqual(["2"]);
    expect(filterDirectory(PEOPLE, "bram@example.test").map((p) => p.id)).toEqual(["2"]);
    // Two words that each match somebody, but nobody who matches both.
    expect(filterDirectory(PEOPLE, "kowalski platform")).toEqual([]);
  });

  test("the order is the server's, never re-sorted by what was typed", () => {
    expect(filterDirectory(PEOPLE, "a").map((p) => p.id)).toEqual(["1", "2", "3"]);
  });
});

describe("who somebody reports to", () => {
  const index = byId(PEOPLE);

  test("is the name, from the answer the rows came from", () => {
    expect(managerName(PEOPLE[1] as HrDirectoryEntry, index)).toBe("Ada Kowalski");
  });

  test("is unknown at the top and unknown when they are not in the answer", () => {
    expect(managerName(PEOPLE[0] as HrDirectoryEntry, index)).toBeNull();
    // A manager id we hold no row for is `null` rather than the id itself: an
    // opaque string in a Reports-to column is worse than a dash.
    expect(managerName(PEOPLE[2] as HrDirectoryEntry, index)).toBeNull();
  });
});

describe("narrowing the chart", () => {
  const CHART = [
    node("1", "Ada", [node("2", "Bram", [node("3", "Chiara")], "Engineer", "Platform")]),
    node("4", "Dieter", [], "Driver", "Logistics"),
  ];

  test("keeps the managers above a match, and only them", () => {
    const kept = filterOrg(CHART, "chiara");
    expect(kept).toHaveLength(1);
    expect(kept[0]?.name).toBe("Ada");
    expect(kept[0]?.reports[0]?.name).toBe("Bram");
    expect(kept[0]?.reports[0]?.reports[0]?.name).toBe("Chiara");
    // The branch that matched nothing is gone, roots and all.
    expect(kept.some((n) => n.name === "Dieter")).toBe(false);
  });

  test("a match keeps the people beneath it too", () => {
    const kept = filterOrg(CHART, "ada");
    expect(countOrg(kept)).toBe(3);
  });

  test("matches a role and a team, not only a name", () => {
    expect(filterOrg(CHART, "logistics").map((n) => n.name)).toEqual(["Dieter"]);
    expect(countOrg(filterOrg(CHART, "warehouse"))).toBe(0);
  });

  test("an empty box leaves the tree exactly as it was served", () => {
    expect(filterOrg(CHART, "")).toBe(CHART);
    expect(countOrg(CHART)).toBe(4);
  });
});
