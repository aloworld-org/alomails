// The second-factor screen: a centered card matching the Figma design. TOTP by
// default (six code boxes, auto-submitting when full), with a toggle to a
// single recovery-code field. Both submit through the same handler — the
// provider accepts a TOTP or a single-use recovery code in the OTP field.
import { useState } from "react";
import type { FormEvent } from "react";
import { ArrowLeft, Lock } from "lucide-react";

import { strings } from "../i18n";
import { Button, Card, Input, Spinner } from "../ds";
import { CodeInput } from "./CodeInput";
import styles from "./TwoFactorScreen.module.css";

interface TwoFactorScreenProps {
  onVerify: (code: string) => void;
  onBack: () => void;
  error: string | null;
  submitting: boolean;
}

export function TwoFactorScreen({ onVerify, onBack, error, submitting }: TwoFactorScreenProps) {
  const [mode, setMode] = useState<"totp" | "recovery">("totp");
  const [code, setCode] = useState("");

  function submit(event: FormEvent) {
    event.preventDefault();
    if (code.trim().length > 0) onVerify(code.trim());
  }

  function switchMode(next: "totp" | "recovery") {
    setMode(next);
    setCode("");
  }

  return (
    <div className={styles.page}>
      <Card as="form" pad="lg" className={styles.panel} onSubmit={submit}>
        <span className={styles.mark} aria-hidden="true">
          <Lock strokeWidth={2} />
        </span>
        <h1 className={styles.title}>{strings.twoFactorTitle}</h1>
        <p className={styles.subtitle}>
          {mode === "totp" ? strings.twoFactorSubtitle : strings.twoFactorRecoverySubtitle}
        </p>

        {mode === "totp" ? (
          <CodeInput
            value={code}
            onChange={setCode}
            onComplete={(full) => onVerify(full)}
            disabled={submitting}
            ariaLabel={strings.twoFactorCodeLabel}
          />
        ) : (
          <Input
            className={styles.recovery}
            size="lg"
            type="text"
            autoComplete="one-time-code"
            placeholder={strings.recoveryPlaceholder}
            aria-label={strings.recoveryCodeLabel}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            disabled={submitting}
            autoFocus
          />
        )}

        {error !== null && (
          <p className={styles.error} role="alert">
            {error}
          </p>
        )}

        <Button type="submit" block disabled={submitting || code.trim().length === 0}>
          {submitting ? <Spinner size={16} label={strings.verifying} /> : strings.verify}
        </Button>

        <button
          type="button"
          className={styles.link}
          onClick={() => switchMode(mode === "totp" ? "recovery" : "totp")}
        >
          {mode === "totp" ? strings.useRecoveryCode : strings.useAuthenticator}
        </button>

        <button type="button" className={styles.back} onClick={onBack}>
          <ArrowLeft size={15} />
          <span>{strings.backToSignIn}</span>
        </button>
      </Card>
    </div>
  );
}
