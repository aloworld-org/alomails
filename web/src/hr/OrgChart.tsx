// The org chart (alo HR, ADR 0035, wave B6.08a): who reports to whom, drawn
// from the tree the server folded.
//
// Presentational only — it takes nodes and draws them. Three decisions are worth
// naming, because each is a thing this file deliberately does *not* do:
//
//   - **It does not build the tree.** `GET /hr/org` answers a shape in which
//     somebody whose manager has left is a root rather than a missing branch,
//     and re-deriving that from `managerId` in a browser is how a person
//     disappears from their company's chart.
//   - **It does not collapse.** A company this suite is built for reads its
//     whole structure in one screen, and a chart that opens folded makes finding
//     somebody a series of guesses. Depth is drawn with a rule down the left of
//     each level, so a deep branch is still followable.
//   - **It does not link anywhere yet.** A person's full record is the People
//     tab (admin or HR, a later item); the chart's job is the shape of the
//     company, and every public fact about a colleague is one press away in the
//     list beside it.
//
// Nested lists, not divs: a screen reader announces "list, 4 items" per level,
// which is exactly what the indentation is saying to everybody else.
import { useEffect, useRef } from "react";

import { Avatar } from "../ds";
import { strings } from "../i18n";
import type { HrOrgNode } from "./types";
import styles from "./hr.module.css";

/**
 * One tenant's reporting tree.
 *
 * `highlightId` is the person the address names (`?person=`) — the row somebody
 * pressed in the list to see where they sit. It is marked, and scrolled to once,
 * rather than filtering the chart down to them: the answer to "where do they
 * sit" is the people around them.
 */
export function OrgChart({
  nodes,
  highlightId,
  selfId,
}: {
  nodes: HrOrgNode[];
  highlightId: string | null;
  selfId: string | null;
}) {
  return (
    <div className={styles.orgWrap}>
      <Level nodes={nodes} highlightId={highlightId} selfId={selfId} depth={0} />
    </div>
  );
}

function Level({
  nodes,
  highlightId,
  selfId,
  depth,
}: {
  nodes: HrOrgNode[];
  highlightId: string | null;
  selfId: string | null;
  depth: number;
}) {
  return (
    <ul className={depth === 0 ? styles.orgRoots : styles.orgLevel}>
      {nodes.map((node) => (
        <li key={node.id} className={styles.orgBranch}>
          <Node node={node} highlighted={node.id === highlightId} isSelf={node.id === selfId} />
          {node.reports.length > 0 && (
            <Level
              nodes={node.reports}
              highlightId={highlightId}
              selfId={selfId}
              depth={depth + 1}
            />
          )}
        </li>
      ))}
    </ul>
  );
}

function Node({
  node,
  highlighted,
  isSelf,
}: {
  node: HrOrgNode;
  highlighted: boolean;
  isSelf: boolean;
}) {
  const card = useRef<HTMLDivElement>(null);

  // Bringing the named person into view is the whole reason the address carries
  // them: pressing "in the chart" on a row two hundred people down must land on
  // that person, not at the top of a chart they are somewhere inside.
  useEffect(() => {
    const node = card.current;
    // Guarded because scrolling is a nicety and drawing the chart is not: a
    // runtime without `scrollIntoView` (jsdom, an old embedded browser) must
    // land somebody at the top of the chart, never take the chart away.
    if (!highlighted || node === null || typeof node.scrollIntoView !== "function") return;
    node.scrollIntoView({ block: "center" });
  }, [highlighted]);

  const classes = [styles.orgCard];
  if (highlighted) classes.push(styles.orgCardFound);
  return (
    <div
      ref={card}
      className={classes.join(" ")}
      {...(highlighted ? { "aria-current": "true" as const } : {})}
    >
      <Avatar name={node.name} size="sm" />
      <div className={styles.orgWho}>
        <span className={styles.orgName}>
          <span>{node.name}</span>
          {isSelf && <span className={styles.selfTag}>{strings.hrYou}</span>}
        </span>
        <span className={styles.orgRole}>
          {[node.jobTitle, node.team].filter((part) => part !== "").join(" · ")}
        </span>
      </div>
      {node.reports.length > 0 && (
        <span className={styles.orgReports}>{strings.hrReportsCount(node.reports.length)}</span>
      )}
    </div>
  );
}
