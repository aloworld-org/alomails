// The Insights module (alo Insights, ADR 0037, wave BI1.05) — the workspace
// surface over the `/insights` API: the boards a tenant keeps its numbers on,
// and the grid of charts that answers them.
//
// It is mounted at `/insights/*` by the product surface, so a board lives at
// its own path and a link to one is a link somebody can send. The tab strip is
// the tenant's boards, oldest first — which is why the seeded Business overview
// (BI1.06) will be the tab a workspace opens on without anyone choosing it.
//
// What this file owns is the chrome and the boards themselves; one board is
// `BoardGrid`. Nothing here computes a figure — every number on an Insights
// screen was computed by the server, in the same functions the printed invoice
// and the VAT return use.
import { useEffect, useState } from "react";
import { BarChart3, Plus } from "lucide-react";
import { NavLink, Navigate, Route, Routes, useNavigate } from "react-router-dom";

import { Button, Spinner, useDialogs } from "../ds";
import { strings } from "../i18n";
import { insightsMessage, useInsightsApi } from "./api";
import { BoardGrid } from "./BoardGrid";
import { EmptyState, ErrorBanner } from "./parts";
import { useBoards } from "./useInsights";
import styles from "./InsightsModule.module.css";

export function InsightsModule() {
  const api = useInsightsApi();
  const dialogs = useDialogs();
  const navigate = useNavigate();
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const boards = useBoards(revision);
  const first = boards.dashboards[0];

  // A board made from here is opened straight away: the tenant asked for it,
  // and landing back on the tab strip would make them click the tab they just
  // created.
  const [pending, setPending] = useState<string | null>(null);
  useEffect(() => {
    if (pending === null) return;
    if (!boards.dashboards.some((d) => d.id === pending)) return;
    setPending(null);
    navigate(`/insights/${pending}`, { replace: false });
  }, [boards.dashboards, navigate, pending]);

  function newBoard() {
    void (async () => {
      const name = (
        await dialogs.prompt({
          title: strings.insightsNewBoard,
          message: strings.insightsBoardNamePrompt,
          placeholder: strings.insightsBoardNamePlaceholder,
        })
      )?.trim();
      if (name === undefined || name === "") return;
      try {
        const created = await api.createDashboard(name);
        setError(null);
        setPending(created.id);
        setRevision((r) => r + 1);
      } catch (err) {
        setError(insightsMessage(err, strings.insightsSaveFailed));
      }
    })();
  }

  const banner = error ?? boards.error;

  return (
    <div className={styles.insights}>
      <header className={styles.header}>
        <h1 className={styles.title}>{strings.moduleInsights}</h1>
        <nav className={styles.tabs} aria-label={strings.insightsBoards}>
          {boards.dashboards.map((board) => (
            <NavLink
              key={board.id}
              to={board.id}
              className={({ isActive }) =>
                isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
              }
            >
              {board.name}
            </NavLink>
          ))}
        </nav>
        <Button variant="ghost" onClick={newBoard}>
          <Plus size={15} />
          {strings.insightsNewBoard}
        </Button>
        {boards.loading && <Spinner size={16} />}
      </header>

      {banner !== null && <ErrorBanner message={banner} />}

      <Routes>
        <Route
          index
          element={
            first === undefined ? (
              boards.loading ? null : (
                <div className={styles.page}>
                  <EmptyState
                    Icon={BarChart3}
                    title={strings.insightsNoBoardsTitle}
                    body={strings.insightsNoBoardsBody}
                    cta={strings.insightsNewBoard}
                    onCta={newBoard}
                  />
                </div>
              )
            ) : (
              <Navigate to={first.id} replace />
            )
          }
        />
        <Route
          path=":dashboardId"
          element={<BoardGrid onBoardsChanged={() => setRevision((r) => r + 1)} />}
        />
        <Route path="*" element={<Navigate to="." replace />} />
      </Routes>
    </div>
  );
}
