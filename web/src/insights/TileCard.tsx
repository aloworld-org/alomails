// One card on a board (ADR 0037, wave BI1.05): a pinned question, the figures
// it currently has, and the few things a reader may do to it from here.
//
// The card decides which renderer draws the answer, and it is the one place
// that knows a tile can be in four honest states — waiting for its figures,
// answered, answered with *nothing* (a real answer: nothing was billed), or
// pinned by a newer version of alo than this one and therefore unreadable. The
// last of those still renders, with its reason, because a board that refuses to
// draw because one card is from the future is worse than a board with a
// placeholder on it (`docs/design/insights.md` § Errors).
//
// What a reader may do here is deliberately small: rename it, make it wider or
// narrower, move it, remove it. Changing the *question* is the builder's
// (BI1.06) — and moving is its own request, so renaming can never rearrange a
// board.
import {
  Bot,
  ChevronLeft,
  ChevronRight,
  Maximize2,
  Minimize2,
  MoreHorizontal,
  Pencil,
  Trash2,
} from "lucide-react";

import { Card, Menu, Spinner, cx } from "../ds";
import type { MenuItem } from "../ds";
import { strings } from "../i18n";
import { Figures } from "./Figures";
import { noteText } from "./format";
import type { Tile } from "./types";
import { useTileFigures } from "./useInsights";
import styles from "./InsightsModule.module.css";

/** The widest a tile may sit — the grid's four columns, the store's `span`
 *  rule. Stated here so the card's own controls cannot ask for a width the
 *  server would refuse. */
export const SPAN_MAX = 4;
/** The narrowest. */
export const SPAN_MIN = 1;

/** What a card can be asked to do to itself. Each returns once the server has
 *  answered, so the board re-reads on a change and stays as it was on a
 *  refusal. */
export interface TileActions {
  rename: (tile: Tile) => void;
  resize: (tile: Tile, span: number) => void;
  move: (tile: Tile, direction: -1 | 1) => void;
  remove: (tile: Tile) => void;
  /** Put this chart in focus below the board — or let it go, when it already
   *  is — so its agent stands under the question it is about (A8.4). */
  focus: (tile: Tile) => void;
}

/** The grid class for a tile's stored width, clamped to the columns the grid
 *  actually has. */
function spanClass(span: number): string {
  const columns = Math.min(Math.max(Math.trunc(span), SPAN_MIN), SPAN_MAX);
  return styles[`span${String(columns)}`] ?? "";
}

export function TileCard({
  tile,
  actions,
  canMoveLeft,
  canMoveRight,
  revision,
}: {
  tile: Tile;
  actions: TileActions;
  canMoveLeft: boolean;
  canMoveRight: boolean;
  revision: number;
}) {
  const figures = useTileFigures(tile.id, tile.readable, revision);

  const items: MenuItem[] = [
    {
      key: "agent",
      label: strings.recordAgentPanelToggle,
      icon: <Bot size={15} />,
      onClick: () => actions.focus(tile),
    },
    {
      key: "rename",
      label: strings.insightsRenameTile,
      icon: <Pencil size={15} />,
      onClick: () => actions.rename(tile),
    },
    {
      key: "wider",
      label: strings.insightsWiden,
      icon: <Maximize2 size={15} />,
      disabled: tile.span >= SPAN_MAX,
      onClick: () => actions.resize(tile, tile.span + 1),
    },
    {
      key: "narrower",
      label: strings.insightsNarrow,
      icon: <Minimize2 size={15} />,
      disabled: tile.span <= SPAN_MIN,
      onClick: () => actions.resize(tile, tile.span - 1),
    },
    {
      key: "left",
      label: strings.insightsMoveLeft,
      icon: <ChevronLeft size={15} />,
      divider: true,
      disabled: !canMoveLeft,
      onClick: () => actions.move(tile, -1),
    },
    {
      key: "right",
      label: strings.insightsMoveRight,
      icon: <ChevronRight size={15} />,
      disabled: !canMoveRight,
      onClick: () => actions.move(tile, 1),
    },
    {
      key: "remove",
      label: strings.insightsRemoveTile,
      icon: <Trash2 size={15} />,
      divider: true,
      danger: true,
      onClick: () => actions.remove(tile),
    },
  ];

  return (
    // `pad="none"` because the card lays out its own three regions: the head
    // holds a menu button that has to reach the corner, and the body is where
    // the figure lives. See `ds/Card`.
    <Card
      as="section"
      pad="none"
      className={cx(
        "flex flex-col min-w-0 min-h-[220px]",
        spanClass(tile.span),
      )}
    >
      <header className={styles.tileHead}>
        <h3 className={styles.tileTitle}>{tile.title}</h3>
        {figures.loading && <Spinner size={14} />}
        <Menu
          label={strings.insightsTileActions(tile.title)}
          icon={<MoreHorizontal size={16} />}
          items={items}
        />
      </header>
      <div className={styles.tileBody}>
        {!tile.readable && (
          <div className={styles.placeholder}>
            <p className={styles.placeholderTitle}>
              {strings.insightsUnreadableTitle}
            </p>
            <p className={styles.quiet}>
              {tile.specError ?? strings.insightsUnreadableBody}
            </p>
          </div>
        )}
        {tile.readable && figures.error !== null && (
          <p className={styles.tileError} role="alert">
            {figures.error}
          </p>
        )}
        {tile.readable && figures.error === null && figures.series !== null && (
          <Figures series={figures.series} viz={tile.viz} title={tile.title} />
        )}
      </div>
      {figures.series !== null && (
        <footer className={styles.tileFoot}>
          {figures.series.notes.map((note) => {
            const text = noteText(note);
            return text === null ? null : (
              <p className={styles.quiet} key={note.code}>
                {text}
              </p>
            );
          })}
          {figures.series.truncated && (
            <p className={styles.quiet}>{strings.insightsTruncated}</p>
          )}
        </footer>
      )}
    </Card>
  );
}
