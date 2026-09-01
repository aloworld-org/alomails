import { useState } from "react";
import { AlertCircle, CalendarDays, Clock3, Plus } from "lucide-react";

import { Button, Card, Select, useIsMobile } from "../ds";
import { strings } from "../i18n";
import { dayLabel, dealValue } from "./format";
import { dealAttention } from "./salesFocus";
import type { CrmDeal, CrmStage } from "./types";

interface Props {
  stages: CrmStage[];
  deals: CrmDeal[];
  onOpen: (id: string) => void;
  onMove: (id: string, stage: CrmStage, position: number) => void;
  onAdd: (stageId: string) => void;
}

function stageTone(stage: CrmStage): string {
  if (stage.isWon) return "bg-success";
  if (stage.isLost) return "bg-danger";
  return "bg-accent";
}

export function BoardView({ stages, deals, onOpen, onMove, onAdd }: Props) {
  const [dragId, setDragId] = useState<string | null>(null);
  const [overStage, setOverStage] = useState<string | null>(null);
  const isMobile = useIsMobile();
  const [phoneStageId, setPhoneStageId] = useState<string | null>(null);
  const phoneStage = stages.find((stage) => stage.id === phoneStageId) ?? stages[0] ?? null;
  const visibleStages = isMobile && phoneStage !== null ? [phoneStage] : stages;
  const inColumn = (stageId: string) =>
    deals.filter((deal) => deal.stageId === stageId).sort((left, right) => left.position - right.position);

  function clearDrag() {
    setDragId(null);
    setOverStage(null);
  }

  function dropOnColumn(stage: CrmStage) {
    if (dragId !== null) {
      const column = inColumn(stage.id).filter((deal) => deal.id !== dragId);
      onMove(dragId, stage, (column.at(-1)?.position ?? 0) + 1);
    }
    clearDrag();
  }

  function dropOnCard(stage: CrmStage, targetId: string) {
    if (dragId !== null && dragId !== targetId) {
      const column = inColumn(stage.id).filter((deal) => deal.id !== dragId);
      const index = column.findIndex((deal) => deal.id === targetId);
      const target = column[index];
      if (target !== undefined) {
        const before = column[index - 1]?.position ?? target.position - 1;
        onMove(dragId, stage, (before + target.position) / 2);
      }
    }
    clearDrag();
  }

  return (
    <section aria-label={strings.crmBoard}>
      {isMobile && stages.length > 0 && phoneStage !== null && (
        <div className="mb-4">
          <Select value={phoneStage.id} onChange={(event) => setPhoneStageId(event.target.value)} aria-label={strings.crmStage}>
            {stages.map((stage) => <option key={stage.id} value={stage.id}>{stage.name} ({inColumn(stage.id).length})</option>)}
          </Select>
        </div>
      )}
      <div className="flex min-h-80 items-start gap-5 overflow-x-auto pb-3">
        {visibleStages.map((stage) => {
          const cards = inColumn(stage.id);
          const selected = overStage === stage.id;
          return (
            <section
              key={stage.id}
              className={`flex min-w-72 flex-1 flex-col gap-3 rounded-2xl border p-3 transition-colors ${selected ? "border-accent bg-accent-soft/30" : "border-subtle bg-raised/45"}`}
              onDragOver={(event) => { event.preventDefault(); setOverStage(stage.id); }}
              onDragLeave={() => setOverStage((current) => current === stage.id ? null : current)}
              onDrop={() => dropOnColumn(stage)}
            >
              <header className="flex min-h-10 items-center gap-2 px-1">
                <span className={`size-2.5 shrink-0 rounded-full ${stageTone(stage)}`} aria-hidden="true" />
                <h3 className="m-0 min-w-0 flex-1 truncate text-sm font-semibold text-primary">{stage.name}</h3>
                <span className="grid min-w-7 place-items-center rounded-full bg-surface px-2 py-1 text-xs font-medium tabular-nums text-secondary">{cards.length}</span>
              </header>
              <div className="flex min-h-3 flex-col gap-3" role="list" aria-label={stage.name}>
                {cards.map((deal) => {
                  const attention = dealAttention(deal, new Date());
                  return (
                    <Card
                      as="button"
                      key={deal.id}
                      pad="sm"
                      interactive
                      className={`flex w-full flex-col gap-3 !bg-surface !text-left ${dragId === deal.id ? "opacity-40" : ""}`}
                      role="listitem"
                      draggable
                      onDragStart={() => setDragId(deal.id)}
                      onDragEnd={clearDrag}
                      onDrop={(event) => { event.stopPropagation(); dropOnCard(stage, deal.id); }}
                      onClick={() => onOpen(deal.id)}
                    >
                      <span className="flex items-start gap-2">
                        <strong className="min-w-0 flex-1 text-sm font-semibold leading-5 text-primary">{deal.title}</strong>
                        {attention === "overdue" && <AlertCircle size={16} className="shrink-0 text-danger" aria-label={strings.crmFocusOverdue} />}
                        {attention === "quiet" && <Clock3 size={16} className="shrink-0 text-warning" aria-label={strings.crmFocusQuiet} />}
                      </span>
                      {deal.companyName !== "" && <span className="text-xs text-secondary">{deal.companyName}</span>}
                      <span className="flex flex-wrap items-center gap-3 border-t border-subtle pt-3">
                        <strong className="text-sm font-semibold tabular-nums text-primary">{dealValue(deal)}</strong>
                        {deal.expectedClose !== null && (
                          <span className="ml-auto inline-flex items-center gap-1.5 text-xs tabular-nums text-secondary">
                            <CalendarDays size={14} aria-hidden="true" />{dayLabel(deal.expectedClose)}
                          </span>
                        )}
                      </span>
                      {deal.source !== "" && <span className="w-fit rounded-full bg-raised px-2.5 py-1 text-xs text-secondary">{deal.source}</span>}
                    </Card>
                  );
                })}
              </div>
              <Button variant="ghost" block icon={<Plus />} onClick={() => onAdd(stage.id)}>{strings.crmNewDeal}</Button>
            </section>
          );
        })}
      </div>
    </section>
  );
}
