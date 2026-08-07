// The Insights read model the screens share (ADR 0037, wave BI1.05): which
// boards exist, what is pinned to the open one, and the figures each tile asks
// for.
//
// Three hooks rather than one, because they are three different reads with
// three different lifetimes. The tab strip is one small response that changes
// when a board is made or renamed; a board's tiles change when one is pinned,
// moved or unpinned; and the **figures** are re-read per tile, on their own, so
// a board draws its grid immediately and fills in as answers arrive rather than
// waiting for the slowest question on it.
//
// Nothing is cached across mounts, deliberately — the server stores nothing
// computed either (`docs/design/insights.md` § Dashboards and tiles), and a
// figure a browser kept from ten minutes ago is exactly the stale number the
// whole design refuses.
import { useEffect, useState } from "react";

import { strings } from "../i18n";
import { insightsMessage, useInsightsApi } from "./api";
import type { BoardContents } from "./api";
import type { Dashboard, Gallery, Series } from "./types";

/** The prebuilt questions the gallery offers. */
export interface GalleryList {
  gallery: Gallery;
  loading: boolean;
  error: string | null;
}

/** The gallery, read only once it is wanted: it is the same answer for every
 *  tenant, so a board that nobody opens the picker on never asks for it. */
export function useGallery(open: boolean): GalleryList {
  const api = useInsightsApi();
  const [gallery, setGallery] = useState<Gallery>({ entries: [], overview: [] });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const answer = await api.gallery();
        if (!live) return;
        setGallery(answer);
        setError(null);
      } catch (err) {
        if (live) setError(insightsMessage(err, strings.insightsGalleryLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, open]);

  return { gallery, loading, error };
}

/** The tenant's boards. */
export interface BoardList {
  dashboards: Dashboard[];
  loading: boolean;
  error: string | null;
}

/** Every board the tenant has, oldest first — and the read that hands a tenant
 *  the seeded Business overview on its first visit, so a workspace opening
 *  Insights for the first time lands on live numbers rather than an empty
 *  state. A tenant that deleted every board has none, which is a state it
 *  chose. */
export function useBoards(revision: number): BoardList {
  const api = useInsightsApi();
  const [dashboards, setDashboards] = useState<Dashboard[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const boards = await api.dashboards();
        if (!live) return;
        setDashboards(boards);
        setError(null);
      } catch (err) {
        if (live) setError(insightsMessage(err, strings.insightsLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, revision]);

  return { dashboards, loading, error };
}

/** One board and its tiles. */
export interface BoardView {
  board: BoardContents | null;
  loading: boolean;
  error: string | null;
}

/** The open board, in layout order. `id` of `null` (no board chosen yet) reads
 *  nothing at all rather than guessing which board was meant. */
export function useBoard(id: string | null, revision: number): BoardView {
  const api = useInsightsApi();
  const [board, setBoard] = useState<BoardContents | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (id === null) {
      setBoard(null);
      setLoading(false);
      return;
    }
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const contents = await api.board(id);
        if (!live) return;
        setBoard(contents);
        setError(null);
      } catch (err) {
        if (live) {
          setBoard(null);
          setError(insightsMessage(err, strings.insightsBoardLoadFailed));
        }
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, id, revision]);

  return { board, loading, error };
}

/** One tile's figures. */
export interface TileFigures {
  series: Series | null;
  loading: boolean;
  error: string | null;
}

/**
 * The figures for one tile, evaluated by the server on every read.
 *
 * A tile whose stored question this build cannot read is never asked: its
 * numbers cannot be answered honestly, the server says so with a `422`, and the
 * card shows the reason it already has instead of a failure it went looking
 * for.
 */
export function useTileFigures(id: string, readable: boolean, revision: number): TileFigures {
  const api = useInsightsApi();
  const [series, setSeries] = useState<Series | null>(null);
  const [loading, setLoading] = useState(readable);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!readable) {
      setSeries(null);
      setLoading(false);
      return;
    }
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const answer = await api.tileData(id);
        if (!live) return;
        setSeries(answer);
        setError(null);
      } catch (err) {
        if (live) {
          setSeries(null);
          setError(insightsMessage(err, strings.insightsFiguresFailed));
        }
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, id, readable, revision]);

  return { series, loading, error };
}
