// The conversations a deal belongs to — the module's reason to exist, and the
// place a careless screen would quietly turn a private mailbox into a shared
// one (`docs/design/crm.md` § Deal ↔ mail thread).
//
// So this panel is built around what a link IS: a pointer, holding no message,
// no addresses and no copy of anything. Consequently:
//
//   - **Nothing is linked without a confirm.** Suggestions are proposals, each
//     carrying the address that matched and why, and are asked for by a click —
//     no screen quietly scans somebody's mail on open.
//   - **Opening a conversation is offered only to a reader who holds it.**
//     `readable` is the server's answer for THIS reader; a colleague who does
//     not hold the thread sees that it is linked and who linked it, which is
//     the useful answer ("ask Sam") rather than a silent gap.
//   - **Unlinking is open to the whole tenant**, because a link left by a
//     colleague who has since left would otherwise be permanent — and removing
//     it destroys nothing.
import { useCallback, useEffect, useState } from "react";
import { Link2, Mail, Sparkles, Unlink } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { Button, IconButton, Spinner } from "../ds";
import { strings } from "../i18n";
import { crmMessage, useCrmApi } from "./api";
import { momentLabel } from "./format";
import { ErrorBanner } from "./parts";
import type { DealThread, ThreadSuggestion } from "./types";
import styles from "./CrmModule.module.css";

export function LinkedThreads({ dealId }: { dealId: string }) {
  const api = useCrmApi();
  const navigate = useNavigate();
  const [threads, setThreads] = useState<DealThread[]>([]);
  const [suggestions, setSuggestions] = useState<ThreadSuggestion[] | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setThreads(await api.threads(dealId));
      setError(null);
    } catch (err) {
      setError(crmMessage(err, strings.crmLoadFailed));
    }
  }, [api, dealId]);

  useEffect(() => {
    void load();
    // A new deal starts with no proposals on screen: suggestions are read on
    // request, never on open.
    setSuggestions(null);
  }, [load]);

  async function suggest() {
    setBusy(true);
    try {
      setSuggestions(await api.threadSuggestions(dealId));
      setError(null);
    } catch (err) {
      setError(crmMessage(err, strings.crmSuggestFailed));
    } finally {
      setBusy(false);
    }
  }

  async function link(threadId: string) {
    try {
      await api.linkThread(dealId, threadId);
      setSuggestions((s) => s?.filter((c) => c.threadId !== threadId) ?? null);
      await load();
    } catch (err) {
      setError(crmMessage(err, strings.crmSaveFailed));
    }
  }

  async function unlink(threadId: string) {
    try {
      await api.unlinkThread(dealId, threadId);
      await load();
    } catch (err) {
      setError(crmMessage(err, strings.crmDeleteFailed));
    }
  }

  return (
    <section className="rounded-xl border border-subtle bg-surface p-5 shadow-sm">
      <h3 className="m-0 flex items-center gap-2 text-sm font-semibold text-primary">
        <span className="grid size-8 place-items-center rounded-lg bg-accent-soft text-accent"><Link2 size={16} /></span>
        {strings.crmThreadsTitle}
      </h3>

      {error !== null && <ErrorBanner message={error} />}

      {threads.length === 0 ? (
        <p className="mb-0 mt-4 rounded-lg bg-raised/40 px-4 py-3 text-sm text-secondary">{strings.crmThreadsEmpty}</p>
      ) : (
        <ul className={`${styles.entries} mt-4`} aria-label={strings.crmThreadsTitle}>
          {threads.map((thread) => (
            <li key={thread.threadId} className={styles.entry}>
              <div className={styles.entryHead}>
                <span className={styles.entryKind}>{thread.subject}</span>
                <span className={styles.cardSpacer} />
                <IconButton
                  label={strings.crmThreadUnlink}
                  icon={<Unlink size={14} />}
                  onClick={() => void unlink(thread.threadId)}
                  title={strings.crmThreadUnlink}
                />
              </div>
              <div className={styles.entryMeta}>
                <span>
                  {strings.crmThreadLinkedBy(
                    thread.linkedBy,
                    momentLabel(thread.linkedAt),
                  )}
                </span>
              </div>
              {thread.readable ? (
                <button
                  type="button"
                  className={styles.linkAction}
                  onClick={() =>
                    navigate(
                      `/mail?thread=${encodeURIComponent(thread.threadId)}`,
                    )
                  }
                >
                  <Mail size={13} /> {strings.crmThreadOpenInMail}
                </button>
              ) : (
                <p className="m-0 text-xs text-tertiary">
                  {strings.crmThreadNotYours}
                </p>
              )}
            </li>
          ))}
        </ul>
      )}

      <div className="mt-4 flex items-center gap-3">
        <Button variant="ghost" onClick={() => void suggest()} disabled={busy}>
          <Sparkles size={14} /> {strings.crmThreadSuggest}
        </Button>
        {busy && <Spinner size={16} />}
      </div>

      {suggestions !== null &&
        (suggestions.length === 0 ? (
          <p className="mb-0 mt-4 rounded-lg bg-raised/40 px-4 py-3 text-sm text-secondary">{strings.crmSuggestionsEmpty}</p>
        ) : (
          <ul className={`${styles.entries} mt-4`} aria-label={strings.crmThreadSuggest}>
            {suggestions.map((candidate) => (
              <li key={candidate.threadId} className={styles.entry}>
                <div className={styles.entryHead}>
                  <span className={styles.entryKind}>{candidate.subject}</span>
                  <span className={styles.cardSpacer} />
                  <span className={styles.entryWhen}>
                    {momentLabel(candidate.lastMessageAt)}
                  </span>
                </div>
                <div className={styles.entryMeta}>
                  <span>
                    {candidate.reason === "address"
                      ? strings.crmSuggestionAddress(candidate.matchedAddress)
                      : strings.crmSuggestionDomain(candidate.matchedAddress)}
                  </span>
                </div>
                <button
                  type="button"
                  className={styles.linkAction}
                  onClick={() => void link(candidate.threadId)}
                >
                  <Link2 size={13} /> {strings.crmThreadLink}
                </button>
              </li>
            ))}
          </ul>
        ))}
    </section>
  );
}
