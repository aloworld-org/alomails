// From a candidate to a colleague: the arithmetic-free part of the bridge
// between a hiring board and the directory (alo HR, ADR 0035, wave B6.08c).
//
// Pure, and deliberately small. What this file holds is the three things the
// hire form needs to be *prefilled* honestly, and nothing that decides
// anything:
//
//   - **which word means "they took the job"** — the stage set is the server's
//     (`docs/design/hr.md` § Recruitment-lite), but the bridge has to know
//     which of those words is the one that ends with somebody starting. It
//     fails closed: a vocabulary without this word simply never offers the
//     bridge, rather than guessing from the last column.
//   - **splitting a written name** into the two fields an employee record
//     keeps. A guess, stated as one: it is a *prefill* the person filling the
//     form corrects, never a fact written behind their back.
//   - **whether this address is already somebody's** in the directory. The
//     server has no unique index on a work address and will happily create a
//     second record; this is what lets the form say so before it happens.
//
// Nothing here writes, reads the network, or judges a candidate.
import type { EmployeeDraft, HrApplicant, HrDirectoryEntry, HrOpening } from "./types";

/**
 * The stage that means somebody took the job.
 *
 * The *set* of stages is served and the board draws whatever arrives; this one
 * word is knowledge the bridge cannot do without, because "add them to the
 * directory" is only ever offered about a person who is actually joining.
 */
export const HIRED_STAGE = "hired";

/**
 * The particles a European family name may begin with.
 *
 * Lower-cased on purpose: `van den Berg` and `de Vries` are family names in
 * three parts, while a middle name is capitalised. It is a heuristic for a
 * prefill and not a rule about names — which is why nothing downstream depends
 * on it being right.
 */
const PARTICLES = new Set([
  "van",
  "von",
  "de",
  "den",
  "der",
  "di",
  "da",
  "du",
  "del",
  "della",
  "dos",
  "el",
  "la",
  "le",
  "ten",
  "ter",
  "bin",
  "bint",
  "al",
]);

/** A written name, split into the two fields an employee record keeps. */
export interface SplitName {
  givenName: string;
  familyName: string;
}

/**
 * Splits the one name an application carried into a given and a family name.
 *
 * The last word is the family name, unless a lower-case particle comes earlier
 * — then the family name starts there, so `Jan van den Berg` keeps his family
 * name whole. A single word becomes the given name and leaves the family name
 * empty, which the form then asks for: an employee record needs both, and
 * inventing the second one is not this file's business.
 */
export function splitName(written: string): SplitName {
  const words = written.trim().split(/\s+/).filter((word) => word !== "");
  if (words.length === 0) return { givenName: "", familyName: "" };
  if (words.length === 1) return { givenName: words[0] ?? "", familyName: "" };
  // A particle at the very front (`de Vries` on its own) is part of the only
  // name there is, so the ordinary last-word rule applies to it.
  const start = words.findIndex((word, index) => index > 0 && PARTICLES.has(word.toLowerCase()));
  const cut = start === -1 ? words.length - 1 : start;
  return {
    givenName: words.slice(0, cut).join(" "),
    familyName: words.slice(cut).join(" "),
  };
}

/** What the hire form opens on. Every field is editable: this is what the
 *  record *says*, offered so nobody retypes it, not what it will hold. */
export interface HirePrefill {
  givenName: string;
  familyName: string;
  workEmail: string;
  jobTitle: string;
  team: string;
  contractKind: string;
}

/**
 * The form's opening values for a candidate who took the job.
 *
 * The round supplies the job title, the team and the kind of contract — it is
 * the role that was advertised, which is the role they were hired into. A
 * round that is not this candidate's (a stale `?applicant=` in the address
 * against a board that has moved on) supplies nothing rather than the wrong
 * role: `opening` is checked against the candidate's own `openingId`.
 */
export function hirePrefill(applicant: HrApplicant, opening: HrOpening | null): HirePrefill {
  const round = opening !== null && opening.id === applicant.openingId ? opening : null;
  return {
    ...splitName(applicant.name),
    workEmail: applicant.email ?? "",
    jobTitle: round?.title ?? "",
    team: round?.team ?? "",
    contractKind: round?.employmentKind ?? "",
  };
}

/** The form's fields as somebody left them. */
export interface HireFields extends HirePrefill {
  /** `YYYY-MM-DD`. Required, and never defaulted to today: a start date is a
   *  fact about a contract, and every leave balance is folded from it. */
  startedOn: string;
}

/**
 * The body `POST /hr/employees` is sent, from the fields as they stand.
 *
 * Blank optional fields are left out rather than sent empty, so the record
 * holds what somebody actually stated. The terms travel in the same act — the
 * create route reads `employment` on create only — because a person in the
 * directory with no start date has no leave balance to read, and the screen
 * that would fix that is not built.
 */
export function employeeDraft(fields: HireFields): EmployeeDraft {
  const draft: EmployeeDraft = {
    givenName: fields.givenName.trim(),
    familyName: fields.familyName.trim(),
    employment: { startedOn: fields.startedOn.trim() },
  };
  const email = fields.workEmail.trim();
  if (email !== "") draft.workEmail = email;
  const jobTitle = fields.jobTitle.trim();
  if (jobTitle !== "") draft.employment.jobTitle = jobTitle;
  const team = fields.team.trim();
  if (team !== "") draft.employment.team = team;
  const kind = fields.contractKind.trim();
  if (kind !== "") draft.employment.contractKind = kind;
  return draft;
}

/** Whether the form has enough to be sent: both names, and the day the terms
 *  begin. Everything else the server would refuse it is the server's. */
export function canHire(fields: HireFields): boolean {
  return (
    fields.givenName.trim() !== "" &&
    fields.familyName.trim() !== "" &&
    fields.startedOn.trim() !== ""
  );
}

/**
 * The person already in the directory under this work address, if there is one.
 *
 * The server keeps no unique index on a work address, so a second press of the
 * same button would quietly create a second colleague. This is what the form
 * warns with — it does **not** refuse: somebody who left and came back is a
 * real second record, and only the person filling the form knows which case
 * this is.
 */
export function alreadyInDirectory(
  people: HrDirectoryEntry[],
  workEmail: string,
): HrDirectoryEntry | null {
  const address = workEmail.trim().toLowerCase();
  if (address === "") return null;
  return people.find((person) => person.workEmail.trim().toLowerCase() === address) ?? null;
}
