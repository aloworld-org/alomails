// The sign-in experience (Figma "02 · Login & Onboarding"): a split screen —
// a charcoal brand panel beside the credentials form — that hands off to the
// dedicated Two-factor screen when the account has 2FA. The app owns the form
// (the IdP renders none) and maps provider outcomes to plain error text.
import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Eye, EyeOff, KeyRound } from "lucide-react";
import { Link, useLocation, useNavigate } from "react-router-dom";

import { surface } from "@product";
import { strings } from "../i18n";
import { Button, Spinner, Field, Input } from "../ds";
import { Logo } from "../shell/Logo";
import { signupDomains } from "../signup/api";
import { useAuth } from "./AuthProvider";
import { AuthError } from "./oidcClient";
import { TwoFactorScreen } from "./TwoFactorScreen";
import styles from "./LoginPage.module.css";

type Step = "credentials" | "twofactor";

export function LoginPage() {
  const { signIn } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const redirectTo =
    (location.state as { from?: string } | null)?.from ?? "/mail";

  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [remember, setRemember] = useState(false);
  const [step, setStep] = useState<Step>("credentials");
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  // Show the "create a personal account" link only when personal signup is
  // actually enabled (ADR 0018); dormant deployments never surface it.
  const [signupOn, setSignupOn] = useState(false);
  useEffect(() => {
    let live = true;
    void signupDomains().then((d) => {
      if (live) setSignupOn(d.length > 0);
    });
    return () => {
      live = false;
    };
  }, []);

  async function attempt(otp?: string) {
    setSubmitting(true);
    setError(null);
    setNote(null);
    try {
      await signIn(email, password, otp, remember);
      navigate(redirectTo, { replace: true });
    } catch (err) {
      if (err instanceof AuthError && err.kind === "second_factor") {
        if (otp === undefined) {
          // Credentials were accepted; the account needs a second factor.
          setStep("twofactor");
        } else {
          // A code was submitted and rejected.
          setError(strings.errorBadOtp);
        }
      } else if (err instanceof AuthError) {
        switch (err.kind) {
          case "bad_credentials":
            setError(strings.errorBadCredentials);
            break;
          case "rate_limited":
            setError(strings.errorRateLimited);
            break;
          case "network":
            setError(strings.errorNetwork);
            break;
          default:
            setError(strings.errorGeneric);
        }
      } else {
        setError(strings.errorGeneric);
      }
    } finally {
      setSubmitting(false);
    }
  }

  if (step === "twofactor") {
    return (
      <TwoFactorScreen
        onVerify={(code) => void attempt(code)}
        onBack={() => {
          setStep("credentials");
          setError(null);
        }}
        error={error}
        submitting={submitting}
      />
    );
  }

  function onSubmit(event: FormEvent) {
    event.preventDefault();
    void attempt();
  }

  return (
    <div className={styles.split}>
      <aside className={styles.brand}>
        <div className={styles.brandTop}>
          <Logo size={40} withWordmark onDark />
        </div>
        <div className={styles.brandBody}>
          <h1 className={styles.headline}>{surface.brand.headline()}</h1>
          <p className={styles.brandSub}>{surface.brand.subtitle()}</p>
        </div>
        <div className={styles.brandFooter}>
          <span className={styles.dot} aria-hidden="true" />
          {surface.brand.euBadge()}
        </div>
      </aside>

      <main className={styles.formPanel}>
        <form className={styles.form} onSubmit={onSubmit}>
          <div className={styles.formHead}>
            <h2 className={styles.heading}>{strings.signInHeading}</h2>
            <p className={styles.subtitle}>{strings.signInSubtitle}</p>
          </div>

          <Field label={strings.emailLabel}>
            {(control) => (
              <Input
                {...control}
                size="lg"
                type="email"
                autoComplete="username"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
                placeholder={surface.login.emailPlaceholder()}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                required
                autoFocus
              />
            )}
          </Field>

          <Field label={strings.passwordLabel}>
            {(control) => (
              <div className={styles.passwordWrap}>
                <Input
                  {...control}
                  size="lg"
                  className={styles.passwordInput}
                  type={showPassword ? "text" : "password"}
                  autoComplete="current-password"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                />
                <button
                  type="button"
                  className={styles.reveal}
                  onClick={() => setShowPassword((v) => !v)}
                  aria-label={
                    showPassword ? strings.hidePassword : strings.showPassword
                  }
                  aria-pressed={showPassword}
                  title={
                    showPassword ? strings.hidePassword : strings.showPassword
                  }
                >
                  {showPassword ? <EyeOff size={18} /> : <Eye size={18} />}
                </button>
              </div>
            )}
          </Field>

          <div className={styles.row}>
            <label className={styles.remember}>
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
              />
              <span>{strings.rememberMe}</span>
            </label>
            <Link to="/reset" className={styles.linkButton}>
              {strings.forgotPassword}
            </Link>
          </div>

          {error !== null && (
            <p className={styles.error} role="alert">
              {error}
            </p>
          )}
          {note !== null && <p className={styles.note}>{note}</p>}

          <Button type="submit" block disabled={submitting}>
            {submitting ? (
              <Spinner size={16} label={strings.signingIn} />
            ) : (
              strings.signInButton
            )}
          </Button>

          {surface.login.sso && (
            <>
              <div className={styles.divider}>
                <span className={styles.rule} />
                <span className={styles.or}>{strings.orDivider}</span>
                <span className={styles.rule} />
              </div>

              <button
                type="button"
                className={styles.sso}
                onClick={() => setNote(strings.ssoComingSoon)}
              >
                <KeyRound size={18} />
                <span>{strings.signInWithSso}</span>
              </button>
            </>
          )}

          {signupOn && (
            <p className={styles.signupPrompt}>
              <Link to="/signup" className={styles.linkButton}>
                {strings.signupCreateLink}
              </Link>
            </p>
          )}
        </form>
      </main>
    </div>
  );
}
