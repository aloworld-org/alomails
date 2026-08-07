// The wire shapes of the `/crm` HTTP surface (alo CRM, ADR 0035, wave B2), as
// the server publishes them (`products/mail/alo-jmap/src/crm_*.rs`).
//
// These are read models, not client rules: nothing here validates, defaults or
// computes. A deal's `state` is the server's own derivation and a deal's money
// is the integer cents it stored — the browser formats those two for reading
// and never invents a third truth about either.

/** Where a deal stands, derived by the server from its closing snapshot. */
export type DealState = "open" | "won" | "lost";

/** What a log entry is. A closed vocabulary the server refuses to widen. */
export type ActivityKind = "note" | "call" | "meeting";

/** Why a conversation was proposed: a correspondent *is* one of the deal's
 *  addresses, or shares its (non-free-mail) domain. */
export type SuggestionReason = "address" | "domain";

/** A board. A tenant's first one is seeded by the server on first read. */
export interface CrmPipeline {
  id: string;
  name: string;
  /** What the board is for; empty when unstated. */
  description: string;
  archived: boolean;
  archivedAt: string | null;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

/** A column of a board. The two flags — not the name — are what make a column
 *  mean "closed": renaming "Won" is a rename, not a schema change. */
export interface CrmStage {
  id: string;
  pipelineId: string;
  name: string;
  position: number;
  isWon: boolean;
  isLost: boolean;
  /** `isWon || isLost` — the server's own word for it. */
  closed: boolean;
  archived: boolean;
  archivedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

/** An opportunity: one card on a board. */
export interface CrmDeal {
  id: string;
  pipelineId: string;
  stageId: string;
  title: string;
  /** The billing customer this deal is for, when the company is already one the
   *  tenant invoices; `null` while it is still a lead. */
  customerId: string | null;
  /** A per-user address-book entry — a convenience pointer that legitimately
   *  does not resolve for a colleague who does not hold it. */
  contactId: string | null;
  companyName: string;
  contactName: string;
  contactEmail: string;
  /** Integer cents, always. */
  valueCents: number;
  currency: string;
  /** A day (`YYYY-MM-DD`), not an instant. */
  expectedClose: string | null;
  ownerUserId: string;
  source: string;
  position: number;
  state: DealState;
  closed: boolean;
  lostReason: string | null;
  closedAt: string | null;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

/** The writable fields of a deal. Absent means "leave as it stands"; `null`
 *  clears a nullable link. It cannot move, reposition or close a deal — that is
 *  the move route, so a stale form can never win a deal. */
export interface DealDraft {
  pipelineId?: string;
  stageId?: string;
  title?: string;
  companyName?: string;
  contactName?: string;
  contactEmail?: string;
  valueCents?: number;
  currency?: string;
  expectedClose?: string | null;
  source?: string;
}

/** One entry of a deal's log. Written once: a correction is another note. */
export interface DealActivity {
  id: string;
  dealId: string;
  kind: ActivityKind;
  body: string;
  /** When it *happened* — a call written up in the evening is dated the hour it
   *  took place, and the log is ordered by this. */
  happenedAt: string;
  authorUserId: string;
  createdAt: string;
}

/** A conversation linked to a deal. The link holds no message: `readable` says
 *  whether **this** reader holds the thread in their own mailbox and can
 *  therefore open it in mail. */
export interface DealThread {
  threadId: string;
  subject: string;
  readable: boolean;
  linkedBy: string;
  linkedAt: string;
}

/** A proposed conversation. It becomes a link only on an explicit confirm —
 *  `reason` and `matchedAddress` are what make that confirmation informed. */
export interface ThreadSuggestion {
  threadId: string;
  subject: string;
  reason: SuggestionReason;
  matchedAddress: string;
  lastMessageAt: string;
}

/** The filters the deal list accepts. Every one is sent to the server, which is
 *  strict about all but the owner: an unknown board, column or state is a `422`
 *  there rather than a silently wider list. */
export interface DealFilter {
  pipelineId?: string;
  stageId?: string;
  ownerUserId?: string;
  state?: DealState;
}
