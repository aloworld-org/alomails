// Ask-to-chart (ADR 0037, wave BI1.07): a question in the reader's own words,
// a chart to look at, and a button that pins it.
//
// The order of those three is the whole design (ADR 0034, propose-then-approve).
// Asking stores nothing — the server evaluates the proposed question and hands
// back figures — so a reader sees the real chart, drawn from their real
// documents, before anything is added to a board. Closing this dialog leaves no
// trace: no tile, no draft, nothing to clean up.
//
// Two things are deliberately not done here. The client never inspects, edits
// or repairs the spec: it hands it back to the server exactly as it arrived, so
// the question that gets pinned is the question that was previewed and the same
// write gate validates it. And the caption stored with the tile is the reader's
// own question, not a phrase a model wrote — words on a European product's
// screen come from the reader or from our catalogs, never from a model's idea
// of what language they speak.
//
// The frame is `ds/Modal` and the question box is a `ds/Field` since D2.08 —
// the same change, and for the same reason, as `GalleryDialog` beside it.
import { useId, useState } from "react";
import { Sparkles, X } from "lucide-react";

import { Button, Field, IconButton, Input, Modal, Spinner } from "../ds";
import { strings } from "../i18n";
import { InsightsError, insightsMessage, useInsightsApi } from "./api";
import { Figures } from "./Figures";
import { ErrorBanner } from "./parts";
import type { AskProposal } from "./types";
import styles from "./InsightsModule.module.css";

/** What to tell a reader about a failed ask. A workspace with no model
 *  configured is not an error the reader made, and the server's own code for it
 *  (`ai-unavailable`) is not a sentence — so that one case gets our words, and
 *  every other refusal keeps the server's, which names what it could not do. */
function askMessage(error: unknown): string {
  if (error instanceof InsightsError && error.status === 503) {
    return strings.insightsAskUnavailable;
  }
  return insightsMessage(error, strings.insightsAskFailed);
}

export function AskDialog({
  busy,
  pinError,
  onPin,
  onClose,
}: {
  /** Whether the reader's approval is being written right now. */
  busy: boolean;
  /** Why pinning the approved chart failed, if it did. */
  pinError: string | null;
  onPin: (proposal: AskProposal, title: string) => void;
  onClose: () => void;
}) {
  const api = useInsightsApi();
  const formId = useId();
  const [question, setQuestion] = useState("");
  /** The question the proposal on screen actually answers — kept apart from the
   *  box, so editing the text does not silently relabel the chart below it. */
  const [asked, setAsked] = useState("");
  const [asking, setAsking] = useState(false);
  const [proposal, setProposal] = useState<AskProposal | null>(null);
  const [error, setError] = useState<string | null>(null);

  function ask() {
    const q = question.trim();
    if (q === "" || asking || busy) return;
    setAsking(true);
    setError(null);
    // A new question replaces the old answer rather than sitting under it: two
    // charts on screen with one Pin button is a question about which one.
    setProposal(null);
    void (async () => {
      try {
        const answer = await api.ask(q);
        setProposal(answer);
        setAsked(q);
      } catch (err) {
        setError(askMessage(err));
      } finally {
        setAsking(false);
      }
    })();
  }

  const banner = pinError ?? error;

  return (
    <Modal
      title={strings.insightsAsk}
      onClose={onClose}
      icon={<Sparkles size={19} />}
      actions={
        <IconButton
          label={strings.insightsAskClose}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          <div className="flex-1" />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {proposal === null
              ? strings.dialogCancel
              : strings.insightsAskDiscard}
          </Button>
          {proposal !== null && (
            <Button disabled={busy} onClick={() => onPin(proposal, asked)}>
              {busy && <Spinner size={14} />}
              {strings.insightsAskPin}
            </Button>
          )}
        </>
      }
    >
      <p className="m-0 text-sm text-tertiary">{strings.insightsAskSubtitle}</p>

      <form
        id={formId}
        className="flex flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          ask();
        }}
      >
        {/* The box and the button are one row, so the label sits above the box
            and the button lines up on the field's bottom edge — which is what
            `items-end` is for. `ds/Field` owns the binding between the two;
            the id it generates replaces the hand-written one this had. */}
        <div className="flex items-end gap-2">
          <div className="min-w-0 flex-1">
            <Field label={strings.insightsAskLabel}>
              {(control) => (
                <Input
                  {...control}
                  value={question}
                  placeholder={strings.insightsAskPlaceholder}
                  autoComplete="off"
                  disabled={busy}
                  onChange={(e) => setQuestion(e.target.value)}
                />
              )}
            </Field>
          </div>
          <Button
            type="submit"
            form={formId}
            disabled={question.trim() === "" || asking || busy}
          >
            {strings.insightsAskSubmit}
          </Button>
        </div>
      </form>

      {banner !== null && <ErrorBanner message={banner} />}
      {asking && <Spinner size={18} />}

      {proposal !== null && (
        <section
          className={styles.askPreview}
          aria-label={strings.insightsAskPreview}
        >
          <h3 className={styles.askPreviewTitle}>{asked}</h3>
          <Figures series={proposal.series} viz={proposal.viz} title={asked} />
          {proposal.repaired && (
            <p className={styles.quiet}>{strings.insightsAskRepaired}</p>
          )}
        </section>
      )}
    </Modal>
  );
}
