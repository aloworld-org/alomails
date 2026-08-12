// Handing one website enquiry to the sales board — the motion competitors need
// an export, a CSV or a Zapier step for (ux law 11c), done here in one dialog.
//
// **Nothing the workspace already knows is asked for again** (law 11b). The
// enquirer's name, address and message are shown as facts and travel with the
// handoff from the stored submission; the server takes them from the row
// itself, so there is no field here to mistype them into. What is left is the
// only thing a person actually decides: which board and column the opportunity
// belongs in, what to call it, and what it might be worth.
//
// The boards come from CRM's own route. A reader who may not see CRM gets its
// refusal sentence verbatim instead of an empty dropdown, because "you are not
// allowed to see the sales boards" and "there are no sales boards" are
// different situations with different ways out.
import { useEffect, useState } from "react";
import { Handshake } from "lucide-react";

import { strings } from "../i18n";
import { SitesError, sitesMessage, useSitesApi } from "./api";
import { DialogFrame, Field } from "./parts";
import type { SiteCrmBoard, SiteCrmColumn, SiteLeadLink, SiteSubmission } from "./types";
import styles from "./SitesModule.module.css";

const received = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

/** What to call the opportunity before anybody edits it: the person who wrote
 *  in, or their address when they left no name. Never blank — a card the user
 *  has to name before they may continue is a dialog asking for something a
 *  default could supply (law 12). */
export function handoffTitle(submission: SiteSubmission): string {
  const who =
    submission.senderName.trim() === "" ? submission.senderEmail : submission.senderName.trim();
  return strings.sitesHandoffTitleFor(who);
}

/** Cents from what a person typed in their own decimal convention. Money on
 *  this wire is an integer of cents and never a float; an unreadable field is
 *  worth nothing rather than something invented. */
export function handoffCents(amount: string): number {
  const normalized = amount.trim().replace(/\s/gu, "").replace(",", ".");
  if (normalized === "") return 0;
  const value = Number(normalized);
  return Number.isFinite(value) ? Math.round(value * 100) : 0;
}

export function HandoffDialog({
  siteId,
  submission,
  onClose,
  onLinked,
}: {
  siteId: string;
  submission: SiteSubmission;
  onClose: () => void;
  onLinked: (link: SiteLeadLink) => void;
}) {
  const api = useSitesApi();
  const [boards, setBoards] = useState<SiteCrmBoard[]>([]);
  const [boardId, setBoardId] = useState("");
  const [columns, setColumns] = useState<SiteCrmColumn[]>([]);
  const [columnId, setColumnId] = useState("");
  const [title, setTitle] = useState(() => handoffTitle(submission));
  const [amount, setAmount] = useState("");
  const [currency, setCurrency] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // A refusal from CRM is not this dialog failing; it is the answer.
  const [denial, setDenial] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    void api
      .crmBoards()
      .then((list) => {
        if (!current) return;
        setBoards(list);
        setBoardId((chosen) => (chosen === "" ? (list[0]?.id ?? "") : chosen));
      })
      .catch((err: unknown) => {
        if (!current) return;
        if (err instanceof SitesError && err.status === 403) {
          setDenial(err.detail ?? strings.sitesHandoffCrmDenied);
        } else {
          setError(sitesMessage(err, strings.sitesHandoffBoardsFailed));
        }
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [api]);

  useEffect(() => {
    if (boardId === "") return;
    let current = true;
    void api
      .crmColumns(boardId)
      .then((list) => {
        if (!current) return;
        setColumns(list);
        // A new opportunity is never raised in a column that means won or
        // lost, so the default is the first column that is still in play.
        const open = list.find((column) => !column.closed) ?? list[0];
        setColumnId(open?.id ?? "");
      })
      .catch((err: unknown) => {
        if (current) setError(sitesMessage(err, strings.sitesHandoffBoardsFailed));
      });
    return () => {
      current = false;
    };
  }, [api, boardId]);

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      onLinked(
        await api.createSiteLead(siteId, submission.id, {
          pipelineId: boardId,
          stageId: columnId,
          title: title.trim(),
          companyName: "",
          valueCents: handoffCents(amount),
          // Blank is the honest default for both: the server falls back to the
          // workspace currency and to this website's own address as the
          // source, which are facts rather than guesses.
          currency: currency.trim().toUpperCase(),
          source: "",
        }),
      );
    } catch (err) {
      setError(sitesMessage(err, strings.sitesHandoffFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Handshake}
      title={strings.sitesHandoffTitle}
      subtitle={strings.sitesHandoffSubtitle}
      error={error}
      busy={busy}
      canSubmit={denial === null && boardId !== "" && columnId !== "" && title.trim() !== ""}
      submitLabel={strings.sitesHandoffSubmit}
      onClose={onClose}
      onSubmit={() => void submit()}
    >
      <dl className={styles.handoffKnown}>
        <div>
          <dt>{strings.sitesHandoffFrom}</dt>
          <dd>
            {submission.senderName}
            <a href={`mailto:${submission.senderEmail}`}>{submission.senderEmail}</a>
          </dd>
        </div>
        <div>
          <dt>{strings.sitesForm}</dt>
          <dd>{submission.formName}</dd>
        </div>
        <div>
          <dt>{strings.sitesReceived}</dt>
          <dd>{received.format(new Date(submission.receivedAt))}</dd>
        </div>
      </dl>
      <p className={styles.hint}>{strings.sitesHandoffCarried}</p>

      {denial !== null ? (
        <p className={styles.handoffDenied}>{denial}</p>
      ) : loading ? (
        <p className={styles.hint} role="status">
          {strings.sitesHandoffLoadingBoards}
        </p>
      ) : boards.length === 0 ? (
        <p className={styles.handoffDenied}>{strings.sitesHandoffNoBoards}</p>
      ) : (
        <>
          <Field label={strings.sitesHandoffBoard}>
            <select
              className={styles.input}
              value={boardId}
              onChange={(event) => setBoardId(event.target.value)}
            >
              {boards.map((board) => (
                <option key={board.id} value={board.id}>
                  {board.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label={strings.sitesHandoffColumn}>
            <select
              className={styles.input}
              value={columnId}
              onChange={(event) => setColumnId(event.target.value)}
            >
              {columns.map((column) => (
                <option key={column.id} value={column.id}>
                  {column.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label={strings.sitesHandoffCardTitle}>
            <input
              className={styles.input}
              value={title}
              onChange={(event) => setTitle(event.target.value)}
            />
          </Field>
          <div className={styles.fieldRow}>
            <Field label={strings.sitesHandoffValue} hint={strings.sitesHandoffValueHint}>
              <input
                className={styles.input}
                inputMode="decimal"
                value={amount}
                onChange={(event) => setAmount(event.target.value)}
              />
            </Field>
            <Field label={strings.sitesHandoffCurrency} hint={strings.sitesHandoffCurrencyHint}>
              <input
                className={styles.input}
                value={currency}
                maxLength={3}
                autoCapitalize="characters"
                autoCorrect="off"
                spellCheck={false}
                onChange={(event) => setCurrency(event.target.value)}
              />
            </Field>
          </div>
        </>
      )}
    </DialogFrame>
  );
}
