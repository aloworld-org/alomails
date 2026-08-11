// Reading the people list (alo HR, ADR 0035, wave B6.08a): who matches what
// somebody typed, and who reports to whom.
//
// Pure, and deliberately the whole of the screen's thinking. Nothing here
// fetches, nothing decides access, and nothing invents a fact the server did not
// serve — the reporting tree in particular is folded server-side and only
// *narrowed* here, never rebuilt from `managerId` (`types.ts`, `HrOrgNode`).
//
// The search is the one piece of judgement, and it is a small one: every word
// somebody types must match somewhere in a person's row, in any field and in any
// order. "diallo berlin" finds Amara Diallo of the Berlin team; typing a whole
// name in the wrong order still finds them. It is a substring match rather than
// anything cleverer, because a directory of a few hundred people is read by
// looking, and a fuzzy match that returns the wrong colleague first is worse
// than one that returns nothing.
import type { HrDirectoryEntry, HrOrgNode } from "./types";

/** The words of a search box, lower-cased, in the order typed. An empty or
 *  whitespace-only box is no words at all — which matches everybody. */
export function searchTerms(query: string): string[] {
  return query.toLowerCase().split(/\s+/).filter((word) => word !== "");
}

/** Everything about a person that a search may match: the way their name is
 *  written, the parts it is written from, their role, their team and the two
 *  ways to reach them. Never anything the directory does not already show. */
function haystack(entry: HrDirectoryEntry): string {
  return [
    entry.name,
    entry.givenName,
    entry.familyName,
    entry.preferredName,
    entry.jobTitle,
    entry.team,
    entry.workEmail,
    entry.workPhone,
  ]
    .join(" ")
    .toLowerCase();
}

/** Whether one person answers a search: every word matches somewhere. */
export function matchesEntry(entry: HrDirectoryEntry, terms: string[]): boolean {
  if (terms.length === 0) return true;
  const text = haystack(entry);
  return terms.every((term) => text.includes(term));
}

/**
 * The people a search leaves, in the order they arrived.
 *
 * The order is the server's — family name first — and is never re-sorted here:
 * a list that reorders itself as somebody types is a list they cannot follow
 * with their eyes.
 */
export function filterDirectory(
  entries: HrDirectoryEntry[],
  query: string,
): HrDirectoryEntry[] {
  const terms = searchTerms(query);
  if (terms.length === 0) return entries;
  return entries.filter((entry) => matchesEntry(entry, terms));
}

/** Everybody by id, for the one lookup a row needs: the name of the person
 *  somebody reports to. Built from the same answer the rows came from, so a
 *  manager who is not in it — archived, or left — is honestly unknown rather
 *  than a second request. */
export function byId(entries: HrDirectoryEntry[]): Map<string, HrDirectoryEntry> {
  return new Map(entries.map((entry) => [entry.id, entry]));
}

/**
 * Who this person reports to, as a name to show.
 *
 * `null` for somebody at the top, and `null` too when the manager is not in the
 * answer we hold — an archived colleague while the list shows only the people
 * who are here. The caller shows the same dash for both, because the difference
 * is not the reader's business.
 */
export function managerName(
  entry: HrDirectoryEntry,
  index: Map<string, HrDirectoryEntry>,
): string | null {
  if (entry.managerId === null) return null;
  return index.get(entry.managerId)?.name ?? null;
}

/**
 * The chart a search leaves: every branch that matches, **with the line of
 * managers above it**.
 *
 * A tree cannot be filtered like a list. Dropping the people who do not match
 * would leave a matching person's reports hanging under whoever is left, which
 * says something false about who they work for. Two rules follow, and between
 * them the shape a match sits in survives exactly as the server served it:
 *
 *   - a person who matches is kept **with everybody beneath them** — searching a
 *     team leader's name is how somebody asks to see that team;
 *   - a person who does not match is kept only when somebody beneath them does,
 *     and then only with the branches that led there.
 */
export function filterOrg(nodes: HrOrgNode[], query: string): HrOrgNode[] {
  const terms = searchTerms(query);
  if (terms.length === 0) return nodes;
  return prune(nodes, terms);
}

function prune(nodes: HrOrgNode[], terms: string[]): HrOrgNode[] {
  const kept: HrOrgNode[] = [];
  for (const node of nodes) {
    if (matchesNode(node, terms)) {
      kept.push(node);
      continue;
    }
    const reports = prune(node.reports, terms);
    if (reports.length > 0) kept.push({ ...node, reports });
  }
  return kept;
}

/** Whether a node answers a search. The chart carries less than a directory row
 *  does — a name, a role, a team — and matching on what is not drawn would hide
 *  the reason a branch is on screen. */
function matchesNode(node: HrOrgNode, terms: string[]): boolean {
  const text = `${node.name} ${node.jobTitle} ${node.team}`.toLowerCase();
  return terms.every((term) => text.includes(term));
}

/** How many people are in a chart, however deep it goes — the count the screen
 *  shows beside a narrowed tree, so "3 of 40" is the honest headline rather than
 *  the number of roots. */
export function countOrg(nodes: HrOrgNode[]): number {
  return nodes.reduce((total, node) => total + 1 + countOrg(node.reports), 0);
}
