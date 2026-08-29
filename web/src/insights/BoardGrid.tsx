// One board: the tiles pinned to it, in the order the server keeps them, and
// the handful of things a reader may do to a board from the board itself (ADR
// 0037, wave BI1.05).
//
// Every change here is a request, and the board is re-read from the answer —
// nothing is moved, renamed or resized optimistically. A refusal therefore
// leaves the grid exactly as it was and says why, which is the only way a
// screen and a server cannot end up disagreeing about what is pinned where.
//
// The one arithmetic in this file is a **position**, and it is an ordering
// rather than a quantity: a tile moving one place lands halfway between its new
// neighbours, so exactly one row changes and the rest of the board is
// untouched (the fractional-ordering shape ADR 0022 set for boards). No figure
// on this screen is ever computed here.
import { useCallback, useState } from "react";
import { BarChart3, Plus, RefreshCw, Sparkles } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { RecordAgentPanel } from "../agents";
import { Button, IconButton, Spinner, useDialogs } from "../ds";
import { strings } from "../i18n";
import { insightsMessage, useInsightsApi } from "./api";
import { AskDialog } from "./AskDialog";
import { GalleryDialog } from "./GalleryDialog";
import { BoardBar, EmptyState, ErrorBanner } from "./parts";
import { SPAN_MAX, SPAN_MIN, TileCard } from "./TileCard";
import type { TileActions } from "./TileCard";
import type { AskProposal, GalleryEntry, Tile } from "./types";
import { useBoard } from "./useInsights";
import styles from "./InsightsModule.module.css";

/** Where a tile lands when it moves one place in `direction`: halfway between
 *  the two tiles it ends up between, or one step past the end of the row.
 *  `null` when there is nowhere to go. */
export function positionAfterMove(tiles: Tile[], index: number, direction: -1 | 1): number | null {
  const neighbour = tiles[index + direction];
  if (neighbour === undefined) return null;
  const beyond = tiles[index + 2 * direction];
  if (beyond === undefined) return neighbour.position + direction;
  return (neighbour.position + beyond.position) / 2;
}

export function BoardGrid({ onBoardsChanged }: { onBoardsChanged: () => void }) {
  const { dashboardId } = useParams();
  const api = useInsightsApi();
  const dialogs = useDialogs();
  const navigate = useNavigate();
  /** Bumped by anything that changed what is pinned here. */
  const [revision, setRevision] = useState(0);
  /** Bumped by an explicit refresh: the figures are re-read, the layout is not
   *  disturbed. Nothing computed is cached anywhere, so this is a real re-ask
   *  of the tenant's documents. */
  const [figures, setFigures] = useState(0);
  const [error, setError] = useState<string | null>(null);
  /** Whether the gallery of ready-made questions is open, and whether one of
   *  them is being pinned right now. */
  const [picking, setPicking] = useState(false);
  const [pinning, setPinning] = useState(false);
  const [pinError, setPinError] = useState<string | null>(null);
  /** Whether the ask (BI1.07) is open. It shares `pinning`/`pinError` with the
   *  gallery: both end in the same one request, and only one of them can be
   *  open at a time. */
  const [asking, setAsking] = useState(false);
  /** The chart whose agent is open under the board, if any — one at a time,
   *  and the board's own agent stands there when no chart is (A8.4). */
  const [focused, setFocused] = useState<string | null>(null);
  const view = useBoard(dashboardId ?? null, revision);
  const tiles = view.board?.tiles ?? [];
  const chartInFocus = tiles.find((tile) => tile.id === focused) ?? null;

  const bump = useCallback(() => setRevision((r) => r + 1), []);

  /** Runs one change against the server and re-reads the board from the
   *  answer. A refusal is shown as the server's own sentence. */
  const commit = useCallback(
    async (change: () => Promise<unknown>, fallback: string) => {
      try {
        await change();
        setError(null);
        bump();
      } catch (err) {
        setError(insightsMessage(err, fallback));
      }
    },
    [bump],
  );

  const actions: TileActions = {
    rename: (tile) => {
      void (async () => {
        const name = (
          await dialogs.prompt({
            title: strings.insightsRenameTile,
            message: strings.insightsRenameTilePrompt,
            defaultValue: tile.title,
          })
        )?.trim();
        if (name === undefined || name === "" || name === tile.title) return;
        await commit(() => api.updateTile(tile.id, { title: name }), strings.insightsSaveFailed);
      })();
    },
    resize: (tile, span) => {
      if (span < SPAN_MIN || span > SPAN_MAX) return;
      void commit(() => api.updateTile(tile.id, { span }), strings.insightsSaveFailed);
    },
    move: (tile, direction) => {
      const position = positionAfterMove(tiles, tiles.indexOf(tile), direction);
      if (position === null) return;
      void commit(() => api.moveTile(tile.id, position), strings.insightsSaveFailed);
    },
    remove: (tile) => {
      void (async () => {
        const sure = await dialogs.confirm({
          title: strings.insightsRemoveTile,
          message: strings.insightsRemoveTileConfirm(tile.title),
          confirmLabel: strings.insightsRemoveTile,
          danger: true,
        });
        if (!sure) return;
        await commit(() => api.deleteTile(tile.id), strings.insightsDeleteFailed);
      })();
    },
    focus: (tile) => setFocused((id) => (id === tile.id ? null : tile.id)),
  };

  /** Pins a ready-made question to this board, with the caption the reader was
   *  looking at when they picked it — their words from that moment on. The spec
   *  is the server's own, sent straight back for the write gate to validate. */
  function pin(entry: GalleryEntry, title: string) {
    setPinning(true);
    setPinError(null);
    void (async () => {
      try {
        await api.createTile(dashboardId ?? "", { title, spec: entry.spec, span: entry.span });
        setPicking(false);
        setError(null);
        bump();
      } catch (err) {
        setPinError(insightsMessage(err, strings.insightsSaveFailed));
      } finally {
        setPinning(false);
      }
    })();
  }

  /** Pins the chart the assistant proposed, captioned with the reader's own
   *  question. The spec is the server's own answer, handed straight back for
   *  the write gate to validate — the client never edits a question it did not
   *  write. */
  function pinProposal(proposal: AskProposal, question: string) {
    setPinning(true);
    setPinError(null);
    void (async () => {
      try {
        await api.createTile(dashboardId ?? "", {
          title: question,
          spec: proposal.spec,
          span: proposal.span,
        });
        setAsking(false);
        setError(null);
        bump();
      } catch (err) {
        setPinError(insightsMessage(err, strings.insightsSaveFailed));
      } finally {
        setPinning(false);
      }
    })();
  }

  function openGallery() {
    setPinError(null);
    setPicking(true);
  }

  function openAsk() {
    setPinError(null);
    setAsking(true);
  }

  function renameBoard() {
    const board = view.board?.dashboard;
    if (board === undefined) return;
    void (async () => {
      const name = (
        await dialogs.prompt({
          title: strings.insightsRenameBoard,
          message: strings.insightsBoardNamePrompt,
          defaultValue: board.name,
        })
      )?.trim();
      if (name === undefined || name === "" || name === board.name) return;
      await commit(async () => {
        await api.renameDashboard(board.id, name);
        onBoardsChanged();
      }, strings.insightsSaveFailed);
    })();
  }

  function deleteBoard() {
    const board = view.board?.dashboard;
    if (board === undefined) return;
    void (async () => {
      const sure = await dialogs.confirm({
        title: strings.insightsDeleteBoard,
        message: strings.insightsDeleteBoardConfirm(board.name),
        confirmLabel: strings.insightsDeleteBoard,
        danger: true,
      });
      if (!sure) return;
      try {
        await api.deleteDashboard(board.id);
        onBoardsChanged();
        // Back to the module, which opens whichever board is now first.
        navigate("/insights", { replace: true });
      } catch (err) {
        setError(insightsMessage(err, strings.insightsDeleteFailed));
      }
    })();
  }

  if (view.loading && view.board === null) {
    return (
      <div className={styles.page}>
        <Spinner size={20} />
      </div>
    );
  }

  if (view.board === null) {
    return (
      <div className={styles.page}>
        {view.error !== null && <ErrorBanner message={view.error} />}
      </div>
    );
  }

  return (
    <div className={styles.page}>
      <BoardBar>
        <h2 className={styles.boardName}>{view.board.dashboard.name}</h2>
        <IconButton
          label={strings.insightsRefresh}
          icon={<RefreshCw size={16} />}
          onClick={() => setFigures((f) => f + 1)}
        />
        <Button variant="ghost" onClick={openGallery}>
          <Plus size={15} />
          {strings.insightsAddChart}
        </Button>
        <Button variant="ghost" onClick={openAsk}>
          <Sparkles size={15} />
          {strings.insightsAsk}
        </Button>
        <Button variant="ghost" onClick={renameBoard}>
          {strings.insightsRenameBoard}
        </Button>
        <Button variant="ghost" onClick={deleteBoard}>
          {strings.insightsDeleteBoard}
        </Button>
      </BoardBar>

      {error !== null && <ErrorBanner message={error} />}
      {view.error !== null && <ErrorBanner message={view.error} />}

      {picking && (
        <GalleryDialog
          busy={pinning}
          error={pinError}
          onPick={pin}
          onClose={() => setPicking(false)}
        />
      )}

      {asking && (
        <AskDialog
          busy={pinning}
          pinError={pinError}
          onPin={pinProposal}
          onClose={() => setAsking(false)}
        />
      )}

      {tiles.length === 0 ? (
        <EmptyState
          Icon={BarChart3}
          title={strings.insightsNoTilesTitle}
          body={strings.insightsNoTilesBody}
          cta={strings.insightsAddChart}
          onCta={openGallery}
        />
      ) : (
        <div className={styles.grid}>
          {tiles.map((tile, index) => (
            <TileCard
              key={tile.id}
              tile={tile}
              actions={actions}
              canMoveLeft={index > 0}
              canMoveRight={index < tiles.length - 1}
              revision={figures}
            />
          ))}
        </div>
      )}

      {/* The record in focus under the board it belongs to: one of its charts
          when a reader picked one from its menu, the board itself otherwise.
          A board and a chart are different records with different verbs — a
          chart is a question that can be re-asked over two periods, a board is
          something more can be pinned to — so each says which it is. Neither
          panel is a second Insights: it opens the room where things run. */}
      <div className="mt-5 max-w-3xl">
        {chartInFocus !== null ? (
          <RecordAgentPanel
            product="insights"
            recordKind="tile"
            recordId={chartInFocus.id}
            recordLabel={chartInFocus.title}
            // A pinned chart keeps no provenance: `createdBy` on the board is
            // an account id and the tile carries none at all, so the panel
            // says it does not know rather than printing a subject nobody can
            // follow (A8.4, AW.2).
            origin={null}
          />
        ) : (
          <RecordAgentPanel
            product="insights"
            recordKind="board"
            recordId={view.board.dashboard.id}
            recordLabel={view.board.dashboard.name}
            origin={null}
          />
        )}
      </div>
    </div>
  );
}
