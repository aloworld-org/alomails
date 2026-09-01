// The pipeline board: one column per stage, one card per deal. The **Tasks
// board interaction**, not a second one (`docs/design/crm.md` § Web surface) —
// native HTML5 drag-and-drop, drop on a column to append, drop on a card to
// land above it, and a fractional position so exactly one row changes
// (ADR 0022).
//
// Two things this board deliberately does NOT do:
//
//   - it never sums a column. Value-by-stage is a server-computed report
//     (B2.08) that reports per currency and refuses to convert; a browser
//     adding up the cards would answer a different question and put a wrong
//     number in a heading.
//   - it never decides that a move is a loss. The column's `isLost` flag is the
//     server's, and the reason it then demands is asked for BEFORE the move is
//     sent, so a drag can never produce a half-closed deal.
import { useState } from "react";
import { AlertCircle, CalendarDays, Clock3, Plus } from "lucide-react";

import { Card, Select, useIsMobile } from "../ds";
import { strings } from "../i18n";
import { dayLabel, dealValue } from "./format";
import { dealAttention } from "./salesFocus";
import type { CrmDeal, CrmStage } from "./types";
import styles from "./CrmModule.module.css";

interface Props {
  stages: CrmStage[];
  deals: CrmDeal[];
  onOpen: (id: string) => void;
  /** Commit a move: the target column, and where in it. The caller asks for a
   *  lost reason when the column needs one. */
  onMove: (id: string, stage: CrmStage, position: number) => void;
  onAdd: (stageId: string) => void;
}

export function BoardView({ stages, deals, onOpen, onMove, onAdd }: Props) {
  const [dragId, setDragId] = useState<string | null>(null);
  const [overStage, setOverStage] = useState<string | null>(null);

  // Phone layout: one column at a time, chosen by a stage picker — a board
  // scrolled sideways through 288px columns shows one and hides the count.
  // Cross-stage moves go through the deal itself there; within the column,
  // drag still reorders. Desktop shows every column, untouched.
  const isMobile = useIsMobile();
  const [phoneStageId, setPhoneStageId] = useState<string | null>(null);
  const phoneStage =
    stages.find((s) => s.id === phoneStageId) ?? stages[0] ?? null;
  const visibleStages =
    isMobile && phoneStage !== null ? [phoneStage] : stages;

  const inColumn = (stageId: string) =>
    deals
      .filter((d) => d.stageId === stageId)
      .sort((a, b) => a.position - b.position);

  function clearDrag() {
    setDragId(null);
    setOverStage(null);
  }

  function dropOnColumn(stage: CrmStage) {
    if (dragId !== null) {
      const column = inColumn(stage.id).filter((d) => d.id !== dragId);
      onMove(dragId, stage, (column.at(-1)?.position ?? 0) + 1);
    }
    clearDrag();
  }

  function dropOnCard(stage: CrmStage, targetId: string) {
    if (dragId !== null && dragId !== targetId) {
      const column = inColumn(stage.id).filter((d) => d.id !== dragId);
      const index = column.findIndex((d) => d.id === targetId);
      const target = column[index];
      if (target !== undefined) {
        const before = column[index - 1]?.position ?? target.position - 1;
        onMove(dragId, stage, (before + target.position) / 2);
      }
    }
    clearDrag();
  }

  return (
    <>
      {isMobile && stages.length > 0 && phoneStage !== null && (
        <div className={styles.stagePicker}>
          <Select
            value={phoneStage.id}
            onChange={(e) => setPhoneStageId(e.target.value)}
            aria-label={strings.crmStage}
          >
            {stages.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name} ({inColumn(s.id).length})
              </option>
            ))}
          </Select>
        </div>
      )}
    <div className={styles.board}>
      {visibleStages.map((stage) => {
        const cards = inColumn(stage.id);
        return (
          <div
            key={stage.id}
            className={`${styles.column} ${overStage === stage.id ? styles.columnOver : ""}`}
            onDragOver={(e) => {
              e.preventDefault();
              setOverStage(stage.id);
            }}
            onDragLeave={() => setOverStage((s) => (s === stage.id ? null : s))}
            onDrop={() => dropOnColumn(stage)}
          >
            <div className={styles.columnHead}>
              <span
                className={`${styles.columnDot} ${stage.isWon ? styles.dotWon : stage.isLost ? styles.dotLost : ""}`}
                aria-hidden="true"
              />
              <span className={styles.columnName}>{stage.name}</span>
              <span className={styles.columnCount}>{cards.length}</span>
            </div>
            {/* The cards are their own list, named after the column, so a
                screen reader (and a test) can say which column it is in
                without the "add" button below pretending to be a card. */}
            <div
              className="flex flex-col gap-2 min-h-2"
              role="list"
              aria-label={stage.name}
            >
              {cards.map((deal) => (
                // A deal card is a `ds/Card`: dense padding, and `interactive`
                // because clicking it really does open the deal.
                <Card
                  key={deal.id}
                  pad="sm"
                  interactive
                  className={`flex flex-col gap-1.5 ${dragId === deal.id ? styles.cardDragging : ""}`}
                  role="listitem"
                  draggable
                  onDragStart={() => setDragId(deal.id)}
                  onDragEnd={clearDrag}
                  onDrop={(e) => {
                    e.stopPropagation();
                    dropOnCard(stage, deal.id);
                  }}
                  onClick={() => onOpen(deal.id)}
                >
                  <div className={styles.cardTopline}>
                    <div className={styles.cardTitle}>{deal.title}</div>
                    {dealAttention(deal, new Date()) === "overdue" && (
                      <span className={styles.cardAlert} title={strings.crmFocusOverdue}>
                        <AlertCircle size={14} />
                      </span>
                    )}
                    {dealAttention(deal, new Date()) === "quiet" && (
                      <span className={styles.cardQuiet} title={strings.crmFocusQuiet}>
                        <Clock3 size={14} />
                      </span>
                    )}
                  </div>
                  {deal.companyName !== "" && (
                    <div className={styles.cardCompany}>{deal.companyName}</div>
                  )}
                  <div className={styles.cardMeta}>
                    <span className={styles.cardValue}>{dealValue(deal)}</span>
                    <span className={styles.cardSpacer} />
                    {deal.expectedClose !== null && (
                      <span className={styles.cardDue}>
                        <CalendarDays size={13} />
                        {dayLabel(deal.expectedClose)}
                      </span>
                    )}
                  </div>
                  {deal.source !== "" && <div className={styles.cardSource}>{deal.source}</div>}
                </Card>
              ))}
            </div>
            <button
              type="button"
              className={styles.cardAdd}
              onClick={() => onAdd(stage.id)}
            >
              <Plus size={15} /> {strings.crmNewDeal}
            </button>
          </div>
        );
      })}
    </div>
    </>
  );
}
