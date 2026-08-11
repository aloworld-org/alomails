// The pure edges of the bridge from a hiring board to the directory: how a
// written name is split, what the form opens on, what it sends, and when it
// warns that an address is already somebody's.
import { describe, expect, test } from "vitest";

import {
  alreadyInDirectory,
  canHire,
  employeeDraft,
  hirePrefill,
  splitName,
  HIRED_STAGE,
} from "./hire";
import type { HrApplicant, HrDirectoryEntry, HrOpening } from "./types";

const OPENING: HrOpening = {
  id: "op-1",
  title: "Backend engineer",
  team: "Platform",
  location: "Rotterdam",
  employmentKind: "fixed_term",
  status: "open",
  openedOn: "2026-08-01",
  closedOn: null,
  applicants: 1,
  createdBy: "u-1",
  createdAt: "2026-08-01T09:00:00Z",
  updatedAt: "2026-08-01T09:00:00Z",
};

function candidate(name: string, email: string | null = "amara@example.test"): HrApplicant {
  return {
    id: "ap-1",
    openingId: OPENING.id,
    name,
    email,
    phone: "",
    source: "referral",
    stage: HIRED_STAGE,
    cvNodeId: null,
    cvFileName: null,
    cvSize: null,
    cvTrashed: false,
    retainUntil: "2027-02-01",
    retentionExpired: false,
    createdAt: "2026-08-02T09:00:00Z",
    updatedAt: "2026-08-02T09:00:00Z",
  };
}

function colleague(over: Partial<HrDirectoryEntry>): HrDirectoryEntry {
  return {
    id: "em-1",
    name: "Bas Jansen",
    givenName: "Bas",
    familyName: "Jansen",
    preferredName: "",
    workEmail: "bas@example.test",
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

describe("splitting a written name", () => {
  test("the last word is the family name", () => {
    expect(splitName("Inès Dupont")).toEqual({ givenName: "Inès", familyName: "Dupont" });
  });

  test("a European particle keeps the family name whole", () => {
    // The reason this heuristic exists at all: `van den Berg` is one family
    // name in three words, and the last-word rule would file him under Berg
    // with "Jan van den" as a given name.
    expect(splitName("Jan van den Berg")).toEqual({
      givenName: "Jan",
      familyName: "van den Berg",
    });
    expect(splitName("Marie de Vries")).toEqual({ givenName: "Marie", familyName: "de Vries" });
  });

  test("a particle at the front is part of the only name there is", () => {
    expect(splitName("de Vries")).toEqual({ givenName: "de", familyName: "Vries" });
  });

  test("three ordinary words put the middle one with the given name", () => {
    expect(splitName("Amara Nnenna Diallo")).toEqual({
      givenName: "Amara Nnenna",
      familyName: "Diallo",
    });
  });

  test("one word leaves the family name for the form to ask for", () => {
    // The record needs both, and inventing the second is not this file's
    // business: the form's submit stays shut until somebody types one.
    expect(splitName("Prince")).toEqual({ givenName: "Prince", familyName: "" });
    expect(canHire({ ...splitName("Prince"), ...blankFields(), startedOn: "2026-09-01" })).toBe(
      false,
    );
  });

  test("stray spacing changes nothing", () => {
    expect(splitName("  Inès   Dupont  ")).toEqual({ givenName: "Inès", familyName: "Dupont" });
    expect(splitName("   ")).toEqual({ givenName: "", familyName: "" });
  });
});

/** The non-name half of the form, empty. */
function blankFields() {
  return { workEmail: "", jobTitle: "", team: "", contractKind: "" };
}

describe("what the hire form opens on", () => {
  test("the round supplies the role, the team and the kind of contract", () => {
    expect(hirePrefill(candidate("Amara Diallo"), OPENING)).toEqual({
      givenName: "Amara",
      familyName: "Diallo",
      workEmail: "amara@example.test",
      jobTitle: "Backend engineer",
      team: "Platform",
      contractKind: "fixed_term",
    });
  });

  test("a round that is not this candidate's supplies nothing", () => {
    // A stale `?applicant=` in the address against a board that has moved on
    // must not fill in the wrong role — an empty field is a question, a wrong
    // one is an error nobody reads.
    const other = { ...OPENING, id: "op-2" };
    expect(hirePrefill(candidate("Amara Diallo"), other)).toMatchObject({
      jobTitle: "",
      team: "",
      contractKind: "",
    });
    expect(hirePrefill(candidate("Amara Diallo"), null)).toMatchObject({ jobTitle: "" });
  });

  test("a candidate with no address opens with an empty one, never null", () => {
    expect(hirePrefill(candidate("Amara Diallo", null), OPENING).workEmail).toBe("");
  });
});

describe("what the form sends", () => {
  test("the person and the terms travel in one body", () => {
    expect(
      employeeDraft({
        givenName: " Amara ",
        familyName: " Diallo ",
        workEmail: " amara@example.test ",
        jobTitle: " Backend engineer ",
        team: " Platform ",
        contractKind: "fixed_term",
        startedOn: " 2026-09-01 ",
      }),
    ).toEqual({
      givenName: "Amara",
      familyName: "Diallo",
      workEmail: "amara@example.test",
      employment: {
        startedOn: "2026-09-01",
        jobTitle: "Backend engineer",
        team: "Platform",
        contractKind: "fixed_term",
      },
    });
  });

  test("blank optional fields are left out, not sent empty", () => {
    // Absent means "the record does not say"; an empty string is a stated
    // blank, and a directory full of stated blanks is a worse record.
    expect(
      employeeDraft({ givenName: "Prince", familyName: "Nelson", ...blankFields(), startedOn: "2026-09-01" }),
    ).toEqual({
      givenName: "Prince",
      familyName: "Nelson",
      employment: { startedOn: "2026-09-01" },
    });
  });

  test("both names and a start date are what the form needs", () => {
    const fields = {
      givenName: "Amara",
      familyName: "Diallo",
      ...blankFields(),
      startedOn: "2026-09-01",
    };
    expect(canHire(fields)).toBe(true);
    expect(canHire({ ...fields, startedOn: "  " })).toBe(false);
    expect(canHire({ ...fields, familyName: "" })).toBe(false);
  });
});

describe("an address that is already somebody's", () => {
  test("is found however either side is written", () => {
    const found = alreadyInDirectory(
      [colleague({ workEmail: " BAS@Example.test " })],
      "bas@example.test",
    );
    expect(found?.name).toBe("Bas Jansen");
  });

  test("is not a colleague who merely has no address", () => {
    // Two people with no work address are not the same person, and a blank
    // matching a blank would warn about every second hire.
    expect(alreadyInDirectory([colleague({ workEmail: "" })], "")).toBeNull();
    expect(alreadyInDirectory([colleague({ workEmail: "" })], "amara@example.test")).toBeNull();
  });

  test("finds somebody who has left, so the form can say so differently", () => {
    const found = alreadyInDirectory(
      [colleague({ archived: true })],
      "bas@example.test",
    );
    expect(found?.archived).toBe(true);
  });
});
