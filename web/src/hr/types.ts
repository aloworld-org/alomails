// What `/hr` answers about hiring, as the screens read it (alo HR, ADR 0035,
// wave B6.06b).
//
// These are the server's own JSON shapes, named — not a model of our own. Two
// vocabularies show why that matters:
//
//   - **stages are served, never assumed.** `GET …/applicants` answers a
//     `stages` list in board order, and the board draws exactly those columns.
//     A build that gains an eighth stage grows a column here without a web
//     release (`docs/design/hr.md` § Recruitment-lite).
//   - **employment kinds are a fixed list**, because the API does not serve
//     one. The picker offers the six the store accepts and the server refuses
//     anything else with a `422` naming the set — so a stale client shows a
//     sentence rather than writing a word nobody knows.

/** Where an opening stands. `closed` is terminal and freezes the record. */
export type OpeningStatus = "draft" | "open" | "closed";

/** The contract an opening is for. The six words the store accepts, in the
 *  order a picker reads best; unknown words that arrive from a newer server are
 *  shown verbatim rather than hidden. */
export const EMPLOYMENT_KINDS = [
  "permanent",
  "fixed_term",
  "part_time",
  "apprentice",
  "contractor",
  "intern",
] as const;

/** A role a tenant is hiring for. */
export interface HrOpening {
  id: string;
  title: string;
  team: string;
  location: string;
  employmentKind: string;
  status: OpeningStatus;
  /** `YYYY-MM-DD`, set when the round was published. */
  openedOn: string | null;
  /** `YYYY-MM-DD`, set when the round ended. */
  closedOn: string | null;
  /** How many people applied — the server's count, never a length of a list
   *  this screen happens to hold. */
  applicants: number;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

/** Somebody who applied. `stage` is the served vocabulary's word, and it moves
 *  only through the move route. */
export interface HrApplicant {
  id: string;
  openingId: string;
  name: string;
  email: string | null;
  phone: string;
  source: string;
  stage: string;
  /** The Drive node their CV became, in the tenant's HR-only area. Never read
   *  by anything — see `docs/design/hr.md` § The EU AI Act posture. */
  cvNodeId: string | null;
  cvFileName: string | null;
  cvSize: number | null;
  /** True when the file has since been trashed in Drive; the record stays as
   *  the honest statement that there was one. */
  cvTrashed: boolean;
  /** `YYYY-MM-DD`: the day after which this record may be erased. */
  retainUntil: string;
  /** The server's own reading of `retainUntil` against today — never recomputed
   *  here, because a browser's clock is not the record's. */
  retentionExpired: boolean;
  createdAt: string;
  updatedAt: string;
}

/** What somebody who met the candidate wrote, with their id on it. */
export interface HrApplicantNote {
  id: string;
  author: string;
  body: string;
  createdAt: string;
}

/** One opening's pipeline, with the vocabulary its columns are drawn from. */
export interface HrPipeline {
  applicants: HrApplicant[];
  stages: string[];
}

/** One candidate, everything written about them, and where they can be moved
 *  to. */
export interface HrApplicantDetail {
  applicant: HrApplicant;
  notes: HrApplicantNote[];
  stages: string[];
}

/** The writable fields of an opening. An absent field keeps what is stored. */
export interface OpeningDraft {
  title?: string;
  team?: string;
  location?: string;
  employmentKind?: string;
}

/** The writable fields of an application. `email: null` clears the address;
 *  `stage` is deliberately absent — moving somebody is its own act. */
export interface ApplicantDraft {
  name?: string;
  email?: string | null;
  phone?: string;
  source?: string;
  retainUntil?: string;
}
