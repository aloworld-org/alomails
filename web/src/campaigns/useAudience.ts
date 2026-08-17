// The audience read model: the count and the people, both asked of whatever
// question is on screen right now.
//
// **The count is a separate read from the list, and deliberately so.** The
// screen states "412 of 500 will be mailed" over the whole audience while the
// table below shows one page of it, and a browser that added up the rows it
// happened to have loaded would report a number that shrinks as you scroll.
//
// **The question is debounced, the answer is not merged.** Typing `B`, `BE`
// starts two reads, and the older one can land second; every effect therefore
// carries a generation and a late answer for a question nobody is asking any
// more is dropped rather than shown. A count that flickers back to the previous
// question is worse than a count that arrives a moment later, because it is a
// number somebody may act on.
import { useCallback, useEffect, useRef, useState } from "react";

import { strings } from "../i18n";
import { campaignsMessage, useCampaignsApi } from "./api";
import type { AudienceMember, SegmentConditions, SegmentTally } from "./types";

/** How long to wait after the last keystroke before asking again. Long enough
 *  that typing "BE, NL" is one question rather than five, short enough that the
 *  number still feels attached to the field. */
const SETTLE_MS = 350;

/** How many people one page of the table holds. */
export const PAGE_SIZE = 50;

/** What the screen knows about the question currently on it. */
export interface AudienceView {
  /** The count and its exclusions, or `null` until the first answer arrives. */
  tally: SegmentTally | null;
  /** The people the question selects, mailable or not, in address order. */
  people: AudienceMember[];
  /** Whether another page may exist — a full page came back, so there may be
   *  more. Never a total: the audience is a live query and a remembered total
   *  goes stale between pages. */
  hasMore: boolean;
  loading: boolean;
  error: string | null;
  /** Asks for the next page, continuing after the last address shown. */
  more: () => void;
  /** Re-reads both, after a failure or after evidence changed. */
  reload: () => void;
}

/**
 * The audience under one question.
 *
 * `conditions` is compared by value, not by identity, so a caller may build a
 * fresh object on every render (which a controlled form does) without sending
 * the same question over and over.
 */
export function useAudience(conditions: SegmentConditions): AudienceView {
  const api = useCampaignsApi();
  const asked = JSON.stringify(conditions);
  const [tally, setTally] = useState<SegmentTally | null>(null);
  const [people, setPeople] = useState<AudienceMember[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  // The page the table has walked to. Reset by the question changing, which is
  // the only correct answer: a cursor from the previous question points into a
  // list that no longer exists.
  const [after, setAfter] = useState<string | null>(null);
  const generation = useRef(0);

  useEffect(() => {
    setAfter(null);
  }, [asked]);

  useEffect(() => {
    const mine = ++generation.current;
    const question = JSON.parse(asked) as SegmentConditions;
    const timer = setTimeout(() => {
      setLoading(true);
      void (async () => {
        try {
          const [counted, page] = await Promise.all([
            api.tally(question),
            api.audience(question, {
              ...(after === null ? {} : { after }),
              limit: PAGE_SIZE,
            }),
          ]);
          if (generation.current !== mine) return;
          setTally(counted);
          // Appending only when continuing: a fresh question replaces the list,
          // and a reload of page one replaces it too.
          setPeople((shown) => (after === null ? page : [...shown, ...page]));
          setHasMore(page.length === PAGE_SIZE);
          setError(null);
        } catch (err) {
          if (generation.current !== mine) return;
          setError(campaignsMessage(err, strings.campaignsLoadFailed));
        } finally {
          if (generation.current === mine) setLoading(false);
        }
      })();
      // Continuing a walk is not a keystroke: only a changed question waits.
    }, after === null ? SETTLE_MS : 0);
    return () => clearTimeout(timer);
  }, [api, asked, after, revision]);

  // The cursor is the last address actually on screen, held in a ref so that
  // `more` never has to be rebuilt as the list grows — a button whose identity
  // changed on every page would re-run any effect keyed on it.
  const shown = useRef<AudienceMember[]>([]);
  shown.current = people;

  const more = useCallback(() => {
    const last = shown.current[shown.current.length - 1];
    if (last !== undefined) setAfter(last.address);
  }, []);

  const reload = useCallback(() => {
    setAfter(null);
    setRevision((r) => r + 1);
  }, []);

  return { tally, people, hasMore, loading, error, more, reload };
}
