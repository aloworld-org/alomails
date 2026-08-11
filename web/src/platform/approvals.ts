// What a queue of things waiting for one person's decision looks like, in one
// place (alo HR, ADR 0035, wave B6.07).
//
// Three modules each own a queue an approver has to empty — leave (HR),
// expense claims (Finance), timesheet weeks (Projects) — and each already
// gates its own with the door that belongs beside its data. `docs/design/hr.md`
// § "The approvals inbox" records the decision this file exists to serve: the
// **inbox adds no server route**. It is composed in the browser from the three
// queues that already exist, and every decision still travels the module's own
// already-gated route.
//
// So what crosses a module boundary is this shape and nothing else: no client,
// no rules, no React. A module hands over rows it has already put into words —
// what a week of hours reads as, what a claim's money reads as — because the
// module that owns the record owns how it is spoken about, and an inbox that
// formatted three kinds of record itself would be a fourth place those
// decisions live.
//
// Nothing here is a permission. A queue a caller may not work is simply not in
// the list they are handed (`web/src/hr/queues.ts`), and the server refuses the
// same read again if a stale client asks anyway.

/** Which module a waiting decision came from. */
export type ApprovalKind = "leave" | "expense" | "timesheet";

/**
 * One thing waiting for a decision, as the inbox shows it.
 *
 * Every text field arrives **already in the interface language**, written by
 * the module that owns the record. `figure` is that module's own formatting of
 * the number the decision turns on — money in Finance's cents-safe formatter,
 * hours in Projects' — never a number this inbox could round differently.
 */
export interface Approval {
  /** Which queue it came from — what the row's chip says. */
  kind: ApprovalKind;
  /** The record's id **within its kind**. Two kinds may collide; a row's React
   *  key is `kind` and `id` together. */
  id: string;
  /** Whose it is: an address, or a name when the record carries one. */
  person: string;
  /** What is being decided, in one line. */
  what: string;
  /** A second line qualifying the first — a note somebody wrote, the category a
   *  claim books to. Empty when the record has none. */
  detail: string;
  /** The number the decision turns on, formatted by the owning module. Empty
   *  when the record has no single figure. */
  figure: string;
  /** When it was handed in (RFC 3339), or `null` when the record does not say.
   *  The inbox sorts on this: the oldest wait is the one that has been unfair
   *  the longest. */
  waitingSince: string | null;
  /** Where the full record lives, so a decision can be taken with its context
   *  one press away. */
  href: string;
}

/**
 * One module's queue, as the inbox works it.
 *
 * `list` never filters by person: the module's own route already answered only
 * what this caller may decide, and a second filter here would be a second place
 * that rule lives.
 */
export interface ApprovalQueue {
  kind: ApprovalKind;
  /** What is waiting, in whatever order the module serves it. The inbox
   *  re-sorts the merged result by wait. */
  list: () => Promise<Approval[]>;
  /** Yes. The note is optional in all three modules and is not asked for. */
  approve: (id: string, note?: string) => Promise<void>;
  /** No, with the sentence the person will read. */
  reject: (id: string, note: string) => Promise<void>;
}
