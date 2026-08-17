// The tenant's saved questions.
//
// A separate read from the audience because it changes for a different reason:
// the audience moves whenever a customer, a deal or a form submission lands,
// and the segment list only when somebody saves or forgets one. Folding them
// into one hook would re-read the list on every keystroke of a question.
import { useCallback, useEffect, useState } from "react";

import { strings } from "../i18n";
import { campaignsMessage, useCampaignsApi } from "./api";
import type { CampaignSegment } from "./types";

export interface SegmentsView {
  segments: CampaignSegment[];
  loading: boolean;
  error: string | null;
  reload: () => void;
}

/** The saved questions, by name. An empty list is an ordinary state — a
 *  workspace that has never saved one still has an audience. */
export function useSegments(): SegmentsView {
  const api = useCampaignsApi();
  const [segments, setSegments] = useState<CampaignSegment[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const saved = await api.segments();
        if (!live) return;
        setSegments(saved);
        setError(null);
      } catch (err) {
        if (!live) return;
        setError(campaignsMessage(err, strings.campaignsSegmentsFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, revision]);

  const reload = useCallback(() => setRevision((r) => r + 1), []);

  return { segments, loading, error, reload };
}
