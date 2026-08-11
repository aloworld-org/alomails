// Reading what HR stored: the word for a stage, the word for where a round
// stands, the contract it is for, and the two kinds of date these records
// carry.
//
// Every function here formats ONE stored value for the interface language.
// Nothing is derived: whether a record is past its retention date is the
// server's `retentionExpired`, not a comparison this file could make against a
// browser's clock, and the stage vocabulary is the one the API served.
//
// The day formatter is Billing's. A calendar day was first printed there, with
// the reason it must be read as text and formatted in UTC — a day somebody
// chose has to survive being shown back to them west of Greenwich.
import { formatDocumentDate } from "../billing";
import { getLocale, strings } from "../i18n";
import type { HrOpening, OpeningStatus } from "./types";

/** A calendar day the server wrote as `YYYY-MM-DD`. */
export function dayLabel(day: string | null, fallback = "—"): string {
  return formatDocumentDate(day, getLocale(), fallback);
}

/** An instant the server wrote (RFC 3339), read in the interface language. */
export function momentLabel(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  return at.toLocaleString(getLocale(), { dateStyle: "medium", timeStyle: "short" });
}

/**
 * What a stage is called.
 *
 * The seven the store has today are named; anything else a newer server serves
 * is shown **verbatim** rather than dropped, because a column nobody can read
 * is still better than candidates that quietly do not appear on the board.
 */
export function stageLabel(stage: string): string {
  switch (stage) {
    case "applied":
      return strings.hrStageApplied;
    case "reviewing":
      return strings.hrStageReviewing;
    case "interview":
      return strings.hrStageInterview;
    case "offer":
      return strings.hrStageOffer;
    case "hired":
      return strings.hrStageHired;
    case "rejected":
      return strings.hrStageRejected;
    case "withdrawn":
      return strings.hrStageWithdrawn;
    default:
      return stage;
  }
}

/** Whether a stage ends a candidacy — which is all the board uses it for: the
 *  outcome columns are toned differently so a full board reads at a glance. */
export function isOutcome(stage: string): "good" | "bad" | null {
  if (stage === "hired") return "good";
  if (stage === "rejected" || stage === "withdrawn") return "bad";
  return null;
}

/** Where a round stands. */
export function statusLabel(status: OpeningStatus): string {
  if (status === "draft") return strings.hrStatusDraft;
  if (status === "closed") return strings.hrStatusClosed;
  return strings.hrStatusOpen;
}

/** The contract an opening is for. An unknown word from a newer server is shown
 *  verbatim, for the reason [`stageLabel`] gives. */
export function kindLabel(kind: string): string {
  switch (kind) {
    case "permanent":
      return strings.hrKindPermanent;
    case "fixed_term":
      return strings.hrKindFixedTerm;
    case "part_time":
      return strings.hrKindPartTime;
    case "apprentice":
      return strings.hrKindApprentice;
    case "contractor":
      return strings.hrKindContractor;
    case "intern":
      return strings.hrKindIntern;
    default:
      return kind;
  }
}

/** How one opening reads in the picker: the role, and the team it is in when
 *  somebody wrote one down. The status is a chip beside the picker rather than
 *  more words inside it. */
export function openingLabel(opening: HrOpening): string {
  return opening.team === "" ? opening.title : `${opening.title} · ${opening.team}`;
}
