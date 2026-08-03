// The "why is this in Spam?" banner, shown at the top of the reading pane when
// the open conversation is in the Junk folder. It states only REAL reasons: the
// inbound SPF/DKIM/DMARC verdict the server recorded (alo:authentication). When
// no authentication problem is present it says so plainly rather than inventing
// a cause, and always offers the one-click way out ("Not spam").
import { ShieldAlert } from "lucide-react";

import type { EmailAddress, MessageAuthentication } from "../../jmap";
import { strings } from "../../i18n";
import styles from "./SpamBanner.module.css";

interface SpamBannerProps {
  auth: MessageAuthentication | null | undefined;
  /** The sender of the message the reasons are drawn from. */
  from: EmailAddress[] | null;
  onNotSpam: () => void;
}

/** The domain part of the first sender address, for the reason text. */
function senderDomain(from: EmailAddress[] | null): string | null {
  const email = from?.[0]?.email ?? "";
  const at = email.lastIndexOf("@");
  const domain = at >= 0 ? email.slice(at + 1) : "";
  return domain.length > 0 ? domain : null;
}

/** Real, human reasons this message is untrusted, from the auth verdict. An
 * empty result means no authentication problem was found. */
function reasons(auth: MessageAuthentication | null | undefined, domain: string | null): string[] {
  if (auth == null) return [];
  const out: string[] = [];
  const named = domain ?? strings.spamSenderFallback;
  if (auth.dmarc === "fail") out.push(strings.spamReasonDmarc(named));
  if (auth.dkim === "fail") out.push(strings.spamReasonDkim);
  if (auth.spf === "fail") out.push(strings.spamReasonSpf(named));
  return out;
}

export function SpamBanner({ auth, from, onNotSpam }: SpamBannerProps) {
  const found = reasons(auth, senderDomain(from));

  return (
    <section className={styles.banner} role="alert">
      <ShieldAlert className={styles.icon} aria-hidden />
      <div className={styles.body}>
        <p className={styles.title}>{strings.spamBannerTitle}</p>
        {found.length > 0 ? (
          <ul className={styles.reasons}>
            {found.map((r) => (
              <li key={r}>{r}</li>
            ))}
          </ul>
        ) : (
          <p className={styles.reason}>{strings.spamReasonNone}</p>
        )}
        <p className={styles.hint}>{strings.spamBannerHint}</p>
      </div>
      <button type="button" className={styles.action} onClick={onNotSpam}>
        {strings.notSpam}
      </button>
    </section>
  );
}
