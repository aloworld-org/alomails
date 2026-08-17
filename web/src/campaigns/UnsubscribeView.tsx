// The page at the end of the unsubscribe link (alo Campaigns, ADR 0044 §3,
// wave C2s.2).
//
// The only screen in this product a stranger reaches: no account, no login, no
// rail, nothing to navigate to. Three decisions are visible in the markup and
// worth naming, because each is the difference between an unsubscribe and a
// spam complaint:
//
//  1. TWO CHOICES, THE SAME SIZE. "Stop this kind of mail" and "stop all of
//     it" are both plain buttons. Making the second one quiet — a grey link
//     under the fold — is how a person who cannot find it presses the spam
//     button instead, and that is the signal that ends a sending reputation.
//     When the send named no kind of mail, the server says so and only the
//     second is drawn: a narrower button for a category no send matches would
//     be worse than not offering one.
//  2. ONE PRESS, NO MAZE. There is no "are you sure", no reason survey and no
//     sign-in. The press is the whole interaction, and the answer replaces the
//     card.
//  3. IT SAYS WHAT ACTUALLY HAPPENED. Both choices are irreversible, so the
//     confirmation states plainly which one was taken and that it cannot be
//     undone from here — per `docs/design/ux-principles.md`, an undo we do not
//     have must not be implied.
//
// Nothing on this page names the recipient. A link is forwarded, quoted in
// replies and read by every scanner between the sender and the inbox; the
// server never returns the address, and this file could not print it if it
// wanted to.
import { useEffect, useState } from "react";
import { Check, MailX } from "lucide-react";

import { Button, Spinner } from "../ds";
import { useParams } from "react-router-dom";

import { strings } from "../i18n";
import {
  unsubscribe,
  unsubscribeLink,
  type UnsubscribeLink,
  type UnsubscribeScope,
} from "./unsubscribeApi";
import styles from "./CampaignsModule.module.css";

/** The server's sentence when it sent one, the fallback otherwise. */
function message(reason: unknown, fallback: string): string {
  return reason instanceof Error && reason.message.length > 0
    ? reason.message
    : fallback;
}

export function UnsubscribeView() {
  const { token = "" } = useParams();
  const [link, setLink] = useState<UnsubscribeLink | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<UnsubscribeScope | null>(null);
  const [error, setError] = useState<string | null>(null);

  // A read, and only a read. The server writes nothing here — see
  // `unsubscribeApi.ts` for why that matters more on this route than anywhere
  // else in the product.
  useEffect(() => {
    let cancelled = false;
    void unsubscribeLink(token).then(
      (value) => {
        if (cancelled) return;
        setLink(value);
        setError(null);
        setLoading(false);
      },
      (reason: unknown) => {
        if (cancelled) return;
        setError(message(reason, strings.campaignUnsubscribeUnknownLink));
        setLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [token]);

  async function press(scope: UnsubscribeScope) {
    setBusy(true);
    setError(null);
    try {
      setLink(await unsubscribe(token, scope));
      setDone(scope);
    } catch (reason) {
      setError(message(reason, strings.campaignUnsubscribeFailed));
    } finally {
      setBusy(false);
    }
  }

  const topic = link?.topic ?? null;
  // Already stopped before this visit: somebody pressing a second time, or a
  // second link from the same send. Saying so is what stops them pressing
  // again and then pressing "spam".
  const alreadyStopped = link !== null && done === null && link.stopped;
  const alreadyDeclined =
    link !== null && done === null && !link.stopped && link.topicDeclined;

  return (
    <main className={styles.unsubscribePage}>
      <section
        className={styles.unsubscribeCard}
        aria-labelledby="unsubscribe-title"
      >
        <span className={styles.unsubscribeMark} aria-hidden="true">
          {done !== null || alreadyStopped ? <Check /> : <MailX />}
        </span>

        {loading ? (
          <div className={styles.unsubscribeStatus} role="status">
            <Spinner size={20} />
            <span>{strings.campaignUnsubscribeLoading}</span>
          </div>
        ) : link === null ? (
          <>
            <h1 id="unsubscribe-title">{strings.campaignUnsubscribeUnknownTitle}</h1>
            <p role="alert">{error ?? strings.campaignUnsubscribeUnknownLink}</p>
          </>
        ) : done !== null ? (
          <>
            <h1 id="unsubscribe-title">{strings.campaignUnsubscribeDoneTitle}</h1>
            <p>
              {done === "all"
                ? strings.campaignUnsubscribeDoneAll
                : strings.campaignUnsubscribeDoneTopic(topic ?? "")}
            </p>
            {done === "topic" && (
              // They asked for less, not for nothing, and the page says so
              // rather than letting them believe everything has stopped.
              <p className={styles.unsubscribeNote}>
                {strings.campaignUnsubscribeDoneTopicNote}
              </p>
            )}
            <p className={styles.unsubscribeNote}>
              {strings.campaignUnsubscribeFinalNote}
            </p>
          </>
        ) : (
          <>
            <h1 id="unsubscribe-title">{strings.campaignUnsubscribeTitle}</h1>
            <p>
              {topic === null
                ? strings.campaignUnsubscribeSubtitleUntopiced
                : strings.campaignUnsubscribeSubtitle(topic)}
            </p>

            {alreadyStopped && (
              <div className={styles.unsubscribeStatus} role="status">
                <Check aria-hidden="true" />
                <span>{strings.campaignUnsubscribeAlreadyStopped}</span>
              </div>
            )}
            {alreadyDeclined && (
              <div className={styles.unsubscribeStatus} role="status">
                <Check aria-hidden="true" />
                <span>
                  {strings.campaignUnsubscribeAlreadyDeclined(topic ?? "")}
                </span>
              </div>
            )}

            {!alreadyStopped && (
              <div className={styles.unsubscribeChoices}>
                {/* Offered first because it is the answer most people
                    actually want, and drawn the same size as the other so
                    neither reads as the discouraged one. */}
                {topic !== null && !alreadyDeclined && (
                  <Button
                    block
                    disabled={busy}
                    onClick={() => void press("topic")}
                  >
                    {strings.campaignUnsubscribeStopTopic(topic)}
                  </Button>
                )}
                <Button
                  block
                  variant="secondary"
                  disabled={busy}
                  onClick={() => void press("all")}
                >
                  {strings.campaignUnsubscribeStopAll}
                </Button>
              </div>
            )}

            {error !== null && (
              <p className={styles.unsubscribeError} role="alert">
                {error}
              </p>
            )}
            <p className={styles.unsubscribeNote}>
              {strings.campaignUnsubscribeNoAccountNote}
            </p>
          </>
        )}
      </section>
    </main>
  );
}
