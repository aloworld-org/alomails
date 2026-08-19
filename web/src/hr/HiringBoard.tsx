// The hiring board: one column per stage, one card per person who applied.
//
// It is the **shared board interaction** the Tasks and CRM kanbans already have
// (`docs/design/hr.md` § Recruitment-lite) — native HTML5 drag-and-drop, drop on
// a column to land in it — with one deliberate difference: there is no drop on
// a card and no position, because an applicant record has no order within a
// stage. Two people at `interview` are not ranked, and a board that let a
// recruiter drag one above the other would be a ranking of candidates drawn by
// hand. They read in the order the applications arrived, which is the order the
// server sends.
//
// Three more things this board does NOT do:
//
//   - it never decides a stage exists. The columns are the `stages` the API
//     served with the pipeline; a build that gains one gains a column here.
//   - it never scores, sorts, ranks or highlights a candidate on anything about
//     them. The only tone on a card is the server's own retention flag, which
//     is about a date and not about a person.
//   - it never moves anybody by itself. A drag asks; the card is redrawn from
//     the server's answer, so a refusal leaves the board exactly as it was.
import { useState } from "react";
import { FileText, Mail, Plus } from "lucide-react";

import { Card } from "../ds";
import { strings } from "../i18n";
import { isOutcome, stageLabel } from "./format";
import { StateBadge } from "./parts";
import type { HrApplicant } from "./types";
import styles from "./hr.module.css";

interface Props {
  /** The stage vocabulary, in board order, exactly as the API served it. */
  stages: string[];
  applicants: HrApplicant[];
  /** The column that offers "add a candidate" — the first stage of an open
   *  round. `null` when the round is closed and applications are refused, so
   *  the board never offers an act the server will not take. */
  addStage: string | null;
  onOpen: (id: string) => void;
  onMove: (id: string, stage: string) => void;
  onAdd: () => void;
}

export function HiringBoard({
  stages,
  applicants,
  addStage,
  onOpen,
  onMove,
  onAdd,
}: Props) {
  const [dragId, setDragId] = useState<string | null>(null);
  const [overStage, setOverStage] = useState<string | null>(null);

  const inColumn = (stage: string) =>
    applicants.filter((a) => a.stage === stage);

  function clearDrag() {
    setDragId(null);
    setOverStage(null);
  }

  function dropOn(stage: string) {
    const dragged = applicants.find((a) => a.id === dragId);
    if (dragged !== undefined && dragged.stage !== stage)
      onMove(dragged.id, stage);
    clearDrag();
  }

  return (
    <div className={styles.board}>
      {stages.map((stage) => {
        const cards = inColumn(stage);
        const outcome = isOutcome(stage);
        return (
          <div
            key={stage}
            className={`${styles.column} ${overStage === stage ? styles.columnOver : ""}`}
            onDragOver={(e) => {
              e.preventDefault();
              setOverStage(stage);
            }}
            onDragLeave={() => setOverStage((s) => (s === stage ? null : s))}
            onDrop={() => dropOn(stage)}
          >
            <div className={styles.columnHead}>
              <span
                className={`${styles.columnDot} ${
                  outcome === "good"
                    ? styles.dotHired
                    : outcome === "bad"
                      ? styles.dotClosed
                      : ""
                }`}
                aria-hidden="true"
              />
              <span className={styles.columnName}>{stageLabel(stage)}</span>
              <span className={styles.columnCount}>{cards.length}</span>
            </div>
            {/* The cards are their own list, named after the column, so a screen
                reader (and a test) can say which column somebody is in without
                the "add" button below pretending to be a candidate. */}
            <div
              className="flex flex-col gap-2 min-h-2"
              role="list"
              aria-label={stageLabel(stage)}
            >
              {cards.map((person) => (
                // A candidate's card is a `ds/Card`: dense padding, and
                // `interactive` because the whole surface opens the record.
                <Card
                  key={person.id}
                  role="listitem"
                  pad="sm"
                  interactive
                  className={`flex flex-col gap-1 ${dragId === person.id ? styles.cardDragging : ""}`}
                  draggable
                  onDragStart={() => setDragId(person.id)}
                  onDragEnd={clearDrag}
                  onClick={() => onOpen(person.id)}
                >
                  <div className={styles.cardTitle}>{person.name}</div>
                  {person.email !== null && person.email !== "" && (
                    <div className={styles.cardLine}>
                      <Mail size={12} aria-hidden="true" />
                      {person.email}
                    </div>
                  )}
                  {person.source !== "" && (
                    <div className={styles.cardLine}>{person.source}</div>
                  )}
                  <div className={styles.cardMeta}>
                    {person.cvNodeId !== null && !person.cvTrashed && (
                      <span className={styles.cardCv}>
                        <FileText size={12} aria-hidden="true" />
                        {strings.hrCv}
                      </span>
                    )}
                    <span className={styles.cardSpacer} />
                    {person.retentionExpired && (
                      <StateBadge tone="bad">
                        {strings.hrRetentionExpired}
                      </StateBadge>
                    )}
                  </div>
                </Card>
              ))}
            </div>
            {addStage === stage && (
              <button type="button" className={styles.cardAdd} onClick={onAdd}>
                <Plus size={15} /> {strings.hrAddCandidate}
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
