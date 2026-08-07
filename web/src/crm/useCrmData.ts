// The CRM read model the screens share: which board is open, what its columns
// are, and which deals answer the question currently being asked.
//
// Two hooks rather than one, because the board and the list ask *different*
// questions of the same records: the board shows every card on the board (it is
// the board), while the list narrows by column, owner and state — and does so
// on the SERVER, which is strict about all but the owner. Splitting them keeps
// the list's filters out of the board and the board's completeness out of the
// list, with one selected pipeline shared above both.
import { useCallback, useEffect, useState } from "react";

import { strings } from "../i18n";
import { crmMessage, useCrmApi } from "./api";
import type { CrmDeal, CrmPipeline, CrmStage, DealState } from "./types";

/** The board that is open: its boards, its columns, and how to change either. */
export interface BoardContext {
  pipelines: CrmPipeline[];
  /** The open board, or `null` while the first read is in flight. */
  pipelineId: string | null;
  selectPipeline: (id: string) => void;
  /** The open board's columns, left to right, archived ones excluded. */
  stages: CrmStage[];
  loading: boolean;
  error: string | null;
  /** Re-reads the boards and their columns (after a rename, or a failure). */
  reload: () => void;
}

/**
 * The tenant's boards and the columns of the open one.
 *
 * The first read is also what **seeds** a tenant's first board, so a workspace
 * that has never opened CRM lands on a working board rather than a setup form.
 * A tenant that archived every board legitimately has none: that is an empty
 * state, not an error.
 */
export function useBoardContext(): BoardContext {
  const api = useCrmApi();
  const [pipelines, setPipelines] = useState<CrmPipeline[]>([]);
  const [pipelineId, setPipelineId] = useState<string | null>(null);
  const [stages, setStages] = useState<CrmStage[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const boards = await api.pipelines();
        if (!live) return;
        setPipelines(boards);
        // Keep the open board if it is still there; otherwise open the first.
        setPipelineId((current) =>
          current !== null && boards.some((p) => p.id === current)
            ? current
            : (boards[0]?.id ?? null),
        );
        setError(null);
      } catch (err) {
        if (live) setError(crmMessage(err, strings.crmLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, revision]);

  useEffect(() => {
    if (pipelineId === null) {
      setStages([]);
      return;
    }
    let live = true;
    void (async () => {
      try {
        const columns = await api.stages(pipelineId);
        if (!live) return;
        setStages(columns.filter((s) => !s.archived));
        setError(null);
      } catch (err) {
        if (live) setError(crmMessage(err, strings.crmLoadFailed));
      }
    })();
    return () => {
      live = false;
    };
  }, [api, pipelineId, revision]);

  return {
    pipelines,
    pipelineId,
    selectPipeline: setPipelineId,
    stages,
    loading,
    error,
    reload: useCallback(() => setRevision((r) => r + 1), []),
  };
}

/** What a deal list is narrowed by, beyond the board it is on. */
export interface DealNarrowing {
  stageId?: string;
  ownerUserId?: string;
  state?: DealState;
}

/** The deals answering one question. */
export interface DealList {
  deals: CrmDeal[];
  loading: boolean;
  error: string | null;
}

/**
 * The deals of one board, narrowed by the stated filters.
 *
 * Every filter goes to the server: the browser never hides a row the API said
 * belongs in the answer, so what is counted and what is shown cannot disagree.
 *
 * `revision` is the ONE refresh channel — bumped by whatever changed a deal
 * elsewhere (the drawer, the deal form). A second one (a `reload()` this hook
 * also exposed) meant every edit fetched the same list twice.
 */
export function useDealList(
  pipelineId: string | null,
  narrow: DealNarrowing,
  revision: number,
): DealList {
  const api = useCrmApi();
  const { stageId, ownerUserId, state } = narrow;
  const [deals, setDeals] = useState<CrmDeal[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (pipelineId === null) {
      setDeals([]);
      setLoading(false);
      return;
    }
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const rows = await api.deals({
          pipelineId,
          ...(stageId === undefined ? {} : { stageId }),
          ...(ownerUserId === undefined ? {} : { ownerUserId }),
          ...(state === undefined ? {} : { state }),
        });
        if (!live) return;
        setDeals(rows);
        setError(null);
      } catch (err) {
        if (live) setError(crmMessage(err, strings.crmLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, pipelineId, stageId, ownerUserId, state, revision]);

  return { deals, loading, error };
}
