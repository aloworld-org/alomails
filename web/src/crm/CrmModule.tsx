// The CRM module (alo CRM, ADR 0035, wave B2) — the workspace surface over the
// `/crm` API: the board a tenant works its opportunities on, the same deals as
// a filterable list, and a drawer over either.
//
// It is mounted at `/crm/*` by the product surface, so every path below is
// relative and a deep link survives a page reload — including the open deal,
// which lives in `?deal=` rather than in component state so that a link to a
// deal is a link somebody can send.
//
// The board is loaded here, with the module, because it IS the module's home;
// the list asks its own narrower question of the same records (see
// `useCrmData`). One `revision` counter ties them together: an edit made in the
// drawer bumps it, and whichever view is on screen re-reads.
import { useState } from "react";
import { BarChart3, Handshake, KanbanSquare, List, Plus } from "lucide-react";
import {
  NavLink,
  Navigate,
  Route,
  Routes,
  useSearchParams,
} from "react-router-dom";

import {
  Button,
  ModuleNavigation,
  Select,
  Spinner,
  moduleNavigationItemClassName,
} from "../ds";
import { strings } from "../i18n";
import { crmMessage, useCrmApi } from "./api";
import { BoardView } from "./BoardView";
import { DealDialog } from "./DealDialog";
import { DealDrawer } from "./DealDrawer";
import { ListView } from "./ListView";
import { useLostReason } from "./LostReasonDialog";
import { moveDeal } from "./moveDeal";
import { EmptyState, ErrorBanner } from "./parts";
import { ReportView } from "./ReportView";
import { SalesFocusPanel } from "./SalesFocusPanel";
import { useBoardContext, useDealList } from "./useCrmData";
import type { CrmStage } from "./types";

/** The tabs: the board first — it is what a sales team opens CRM to look at —
 *  then the same deals as a list, for the questions a board cannot answer
 *  ("everything I own that is still open, by value"), then the report, which is
 *  the only screen here that shows a total. */
const TABS = [
  { path: "/crm/board", label: () => strings.crmBoard, Icon: KanbanSquare },
  { path: "/crm/list", label: () => strings.crmList, Icon: List },
  { path: "/crm/report", label: () => strings.crmReport, Icon: BarChart3 },
] as const;

export function CrmModule() {
  const api = useCrmApi();
  const lost = useLostReason();
  const board = useBoardContext();
  const [searchParams, setSearchParams] = useSearchParams();
  const [revision, setRevision] = useState(0);
  /** `null` = closed; a stage id = raising a deal in that column. */
  const [creatingIn, setCreatingIn] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const deals = useDealList(board.pipelineId, {}, revision);
  const openDealId = searchParams.get("deal");

  function openDeal(id: string | null) {
    const next = new URLSearchParams(searchParams);
    if (id === null) next.delete("deal");
    else next.set("deal", id);
    setSearchParams(next, { replace: true });
  }

  /** Something changed a deal: whichever view is on screen re-reads it. */
  function bump() {
    setRevision((r) => r + 1);
  }

  /** A card was dropped: ask for a lost reason if the column needs one, then
   *  commit. A refusal leaves the board exactly as it was — the card is drawn
   *  from the server's answer, never from an optimistic guess. */
  async function commitMove(id: string, stage: CrmStage, position: number) {
    try {
      const moved = await moveDeal(api, lost.ask, id, stage, position);
      if (moved !== null) bump();
    } catch (err) {
      setError(crmMessage(err, strings.crmSaveFailed));
    }
  }

  const banner = error ?? board.error ?? deals.error;

  return (
    <div className="relative flex h-full min-h-0 w-full flex-col bg-app">
      <header className="shrink-0 border-b border-subtle bg-header px-8 pb-5 pt-6 max-sm:px-4">
        <div className="flex flex-wrap items-start gap-5">
          <span
            className="grid size-11 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent"
            aria-hidden="true"
          >
            <Handshake size={21} />
          </span>
          <div className="min-w-0 flex-1">
            <h1 className="m-0 text-2xl font-semibold tracking-tight text-primary">
              {strings.moduleCrm}
            </h1>
            <p className="mb-0 mt-1 text-sm text-secondary">
              {strings.crmWorkspaceSubtitle}
            </p>
          </div>
          {board.pipelines.length > 1 && (
            <Select
              value={board.pipelineId ?? ""}
              onChange={(event) => board.selectPipeline(event.target.value)}
              aria-label={strings.crmPipeline}
            >
              {board.pipelines.map((pipeline) => (
                <option key={pipeline.id} value={pipeline.id}>
                  {pipeline.name}
                </option>
              ))}
            </Select>
          )}
          <Button
            variant="primary"
            icon={<Plus />}
            disabled={board.pipelineId === null || board.stages.length === 0}
            onClick={() => setCreatingIn(board.stages[0]?.id ?? null)}
          >
            {strings.crmNewDeal}
          </Button>
        </div>
        <div className="mt-5 flex items-center gap-3">
          <ModuleNavigation label={strings.moduleCrm} className="flex-1">
            {TABS.map(({ path, label, Icon }) => (
              <NavLink
                key={path}
                to={path}
                className={({ isActive }) =>
                  moduleNavigationItemClassName(isActive)
                }
              >
                <Icon aria-hidden="true" />
                {label()}
              </NavLink>
            ))}
          </ModuleNavigation>
          {(board.loading || deals.loading) && <Spinner size={16} />}
        </div>
      </header>

      {banner !== null && banner !== undefined && (
        <ErrorBanner message={banner} />
      )}

      {board.pipelineId === null && !board.loading ? (
        <EmptyState
          Icon={Handshake}
          title={strings.crmNoBoardTitle}
          body={strings.crmNoBoardBody}
        />
      ) : (
        <Routes>
          <Route index element={<Navigate to="/crm/board" replace />} />
          <Route
            path="board"
            element={
              <div className="mx-auto flex min-h-0 w-full max-w-[112rem] flex-1 flex-col gap-5 overflow-auto px-8 py-6 max-[52rem]:px-4 max-[52rem]:py-4">
                <SalesFocusPanel deals={deals.deals} onOpen={openDeal} />
                <BoardView
                  stages={board.stages}
                  deals={deals.deals}
                  onOpen={openDeal}
                  onMove={(id, stage, position) =>
                    void commitMove(id, stage, position)
                  }
                  onAdd={setCreatingIn}
                />
              </div>
            }
          />
          <Route
            path="list"
            element={
              <ListView
                pipelineId={board.pipelineId}
                stages={board.stages}
                revision={revision}
                onOpen={openDeal}
                onCreate={() => setCreatingIn(board.stages[0]?.id ?? null)}
              />
            }
          />
          <Route
            path="report"
            element={
              <ReportView pipelineId={board.pipelineId} revision={revision} />
            }
          />
          {/* An unknown CRM path is a stale link, not an error page. */}
          <Route path="*" element={<Navigate to="/crm/board" replace />} />
        </Routes>
      )}

      {openDealId !== null && (
        <DealDrawer
          dealId={openDealId}
          stages={board.stages}
          onClose={() => openDeal(null)}
          onChanged={bump}
        />
      )}

      {creatingIn !== null && board.pipelineId !== null && (
        <DealDialog
          deal={null}
          pipelineId={board.pipelineId}
          stageId={creatingIn}
          onClose={() => setCreatingIn(null)}
          onSaved={(created) => {
            setCreatingIn(null);
            bump();
            openDeal(created.id);
          }}
        />
      )}

      {/* The question a losing column asks, rendered once for the board and the
          list behind it — the move that needs it awaits this. */}
      {lost.dialog}
    </div>
  );
}
