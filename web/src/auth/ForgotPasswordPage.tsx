// The self-service password-reset page (ADR 0018 follow-up): the same split
// brand/form frame as sign-in, driving a two-step flow — enter your alo
// address, then the code mailed to your recovery mailbox plus a new password —
// against the unauthenticated /reset/* API. The request step always advances,
// even for an unknown address, so the page never reveals which accounts exist.
import { useState } from "react";
import type { FormEvent } from "react";
import { Eye, EyeOff } from "lucide-react";
import { Link, useNavigate } from "react-router-dom";

import { surface } from "@product";
import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import { Logo } from "../shell/Logo";
import login from "./LoginPage.module.css";
import styles from "../signup/SignupPage.module.css";
import { SignupError, resetRequest, resetVerify } from "../signup/api";

type Step = "request" | "verify" | "done";

export function ForgotPasswordPage() {
  const navigate = useNavigate();
  const [step, setStep] = useState<Step>("request");
  const [address, setAddress] = useState("");
  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function request(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await resetRequest(address.trim());
      // Always advance — the server reveals nothing about whether the account
      // exists, and neither must the UI.
      setStep("verify");
    } catch (e) {
      setError(
        e instanceof SignupError && e.message === "network"
          ? strings.errorNetwork
          : strings.resetRequestError,
      );
    } finally {
      setBusy(false);
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await resetVerify(address.trim(), code.trim(), password);
      setStep("done");
    } catch (e) {
      setError(
        e instanceof SignupError && e.message === "network"
          ? strings.errorNetwork
          : strings.resetVerifyError,
      );
    } finally {
      setBusy(false);
    }
  }

  async function resend() {
    setError(null);
    try {
      await resetRequest(address.trim());
    } catch {
      setError(strings.resetRequestError);
    }
  }

  return (
    <div className={login.split}>
      <aside className={login.brand}>
        <div className={login.brandTop}>
          <Logo size={40} withWordmark onDark />
        </div>
        <div className={login.brandBody}>
          <h1 className={login.headline}>{surface.brand.headline()}</h1>
          <p className={login.brandSub}>{surface.brand.subtitle()}</p>
        </div>
        <div className={login.brandFooter}>
          <span className={login.dot} aria-hidden="true" />
          {surface.brand.euBadge()}
        </div>
      </aside>

      <main className={login.formPanel}>
        {step === "done" ? (
          <div className={login.form}>
            <div className={login.formHead}>
              <h2 className={login.heading}>{strings.resetDoneHeading}</h2>
              <p className={login.subtitle}>{strings.resetDoneBody}</p>
            </div>
            <Button block onClick={() => navigate("/login")}>
              {strings.signupGoToLogin}
            </Button>
          </div>
        ) : step === "verify" ? (
          <form className={login.form} onSubmit={submit}>
            <div className={login.formHead}>
              <h2 className={login.heading}>{strings.resetVerifyHeading}</h2>
              <p className={login.subtitle}>{strings.resetVerifySubtitle(address.trim())}</p>
            </div>

            <label className={login.field}>
              <span className={login.label}>{strings.signupCodeLabel}</span>
              <input
                className={login.input}
                inputMode="numeric"
                autoComplete="one-time-code"
                maxLength={6}
                value={code}
                onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
                required
                autoFocus
              />
            </label>

            <label className={login.field}>
              <span className={login.label}>{strings.resetNewPasswordLabel}</span>
              <div className={login.passwordWrap}>
                <input
                  className={`${login.input} ${login.passwordInput}`}
                  type={showPassword ? "text" : "password"}
                  autoComplete="new-password"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
                  minLength={8}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                />
                <button
                  type="button"
                  className={login.reveal}
                  onClick={() => setShowPassword((v) => !v)}
                  aria-label={showPassword ? strings.hidePassword : strings.showPassword}
                  aria-pressed={showPassword}
                >
                  {showPassword ? <EyeOff size={18} /> : <Eye size={18} />}
                </button>
              </div>
              <span className={styles.hint}>{strings.signupPasswordHint}</span>
            </label>

            {error !== null && (
              <p className={login.error} role="alert">
                {error}
              </p>
            )}

            <Button type="submit" block disabled={busy || code.length < 6 || password.length < 8}>
              {busy ? <Spinner size={16} label={strings.resetSubmitting} /> : strings.resetSubmit}
            </Button>
            <button type="button" className={login.linkButton} onClick={() => void resend()}>
              {strings.signupResend}
            </button>
          </form>
        ) : (
          <form className={login.form} onSubmit={request}>
            <div className={login.formHead}>
              <h2 className={login.heading}>{strings.resetHeading}</h2>
              <p className={login.subtitle}>{strings.resetSubtitle}</p>
            </div>

            <label className={login.field}>
              <span className={login.label}>{strings.resetAddressLabel}</span>
              <input
                className={login.input}
                type="email"
                autoComplete="username"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
                placeholder={surface.login.emailPlaceholder()}
                value={address}
                onChange={(e) => setAddress(e.target.value)}
                required
                autoFocus
              />
            </label>

            {error !== null && (
              <p className={login.error} role="alert">
                {error}
              </p>
            )}

            <Button type="submit" block disabled={busy || address.trim() === ""}>
              {busy ? <Spinner size={16} label={strings.resetSending} /> : strings.resetSendCode}
            </Button>

            <p className={styles.haveAccount}>
              <Link to="/login" className={login.linkButton}>
                {strings.signupBackToLogin}
              </Link>
            </p>
          </form>
        )}
      </main>
    </div>
  );
}
