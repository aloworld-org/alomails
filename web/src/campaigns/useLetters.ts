// The letters this workspace has written, and one of them rendered for one
// reader (alo Campaigns, ADR 0044, wave C3.6).
//
// Two reads that move for different reasons, so they are two effects rather
// than one: the list changes when somebody writes or deletes a letter, and the
// preview changes every time the reader being previewed changes. Folding them
// together would re-fetch every letter each time the "show as" select moves.
//
// **The preview is never assembled here.** The HTML, the text part and the
// merge-field report all come from the server, from the same compilation a send
// would use — a browser that re-rendered any of it would be a second opinion
// about what a customer's customers receive, which is the one thing a preview
// must not be.
import { useEffect, useState } from "react";

import { strings } from "../i18n";
import { campaignsMessage, useCampaignsApi } from "./api";
import type { CampaignPreview, CampaignSummary } from "./types";

export interface LettersView {
  /** Every letter, newest first. */
  letters: CampaignSummary[];
  /** Which one is open, or `""` when there are none. */
  openLetter: string;
  open: (id: string) => void;
  /** The open letter as the chosen reader receives it, or `null` while it is
   *  being read (or when there is no letter to read). */
  preview: CampaignPreview | null;
  /** Whose copy: an address, `PREVIEW_AS_FALLBACKS`, or `""` for whoever this
   *  workspace would mail first. */
  showAs: string;
  setShowAs: (against: string) => void;
  loading: boolean;
  error: string | null;
}

/** The letters, and the open one rendered. */
export function useLetters(): LettersView {
  const api = useCampaignsApi();
  const [letters, setLetters] = useState<CampaignSummary[]>([]);
  const [openLetter, setOpenLetter] = useState("");
  const [preview, setPreview] = useState<CampaignPreview | null>(null);
  const [showAs, setShowAs] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const written = await api.campaigns();
        if (!live) return;
        setLetters(written);
        setError(null);
        // Open the newest by default. A picker that starts on nothing makes the
        // first thing a colleague does a step the screen could have taken.
        setOpenLetter((current) =>
          current !== "" && written.some((letter) => letter.id === current)
            ? current
            : (written[0]?.id ?? ""),
        );
      } catch (err) {
        if (!live) return;
        setError(campaignsMessage(err, strings.campaignsLettersFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api]);

  useEffect(() => {
    if (openLetter === "") {
      setPreview(null);
      return;
    }
    let live = true;
    setLoading(true);
    setPreview(null);
    void (async () => {
      try {
        const rendered = await api.preview(openLetter, showAs);
        if (!live) return;
        setPreview(rendered);
        setError(null);
      } catch (err) {
        if (!live) return;
        setError(campaignsMessage(err, strings.campaignsPreviewFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, openLetter, showAs]);

  return {
    letters,
    openLetter,
    open: setOpenLetter,
    preview,
    showAs,
    setShowAs,
    loading,
    error,
  };
}
