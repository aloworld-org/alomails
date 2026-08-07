// The shape `GET /audit` answers with (alo audit trail, ADR 0035, wave B2.13).
//
// One entry is one act: a verb, the person who did it, and when. It carries no
// before/after values and no request body by design — a log that quotes what
// changed is a second copy of the record, kept somewhere with different access
// rules, which is exactly the leak a sovereignty product cannot afford.

/** One recorded act on one record, as the server sends it. */
export interface AuditEntry {
  id: string;
  /** The dotted verb, e.g. `billing.invoice.issue`. Server vocabulary. */
  action: string;
  /** The acting person's address, or `null` when the account is gone. */
  actor: string | null;
  entityType: string | null;
  entityId: string | null;
  /** The route the act came in on — context for support, not for the user. */
  target: string | null;
  detail: string | null;
  /** RFC 3339 instant. */
  at: string;
}

/** A record addressed the way the audit surface addresses one. */
export interface AuditSubject {
  /** `billing.invoice`, `crm.deal`, … */
  entityType: string;
  entityId: string;
}
