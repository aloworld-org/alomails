// The public personal-signup page (ADR 0018): the same split brand/form frame
// as sign-in, driving the three-step flow — pick an address, verify a code
// sent to a recovery mailbox, set a password — against the unauthenticated
// /signup/* API. When no personal domains are configured the surface is off
// and the page says so.
import { useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent } from "react";
import { Eye, EyeOff } from "lucide-react";
import { Link, useNavigate } from "react-router-dom";

import { surface } from "@product";
import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import { Logo } from "../shell/Logo";
import login from "../auth/LoginPage.module.css";
import styles from "./SignupPage.module.css";
import {
  SignupError,
  signupAvailable,
  signupBegin,
  signupDomains,
  signupVerify,
} from "./api";

type Step = "address" | "verify" | "done";

/** Availability reason → localized message. */
function reasonText(reason: string): string {
  switch (reason) {
    case "ok":
      return strings.signupAvailable;
    case "taken":
      return strings.signupTaken;
    case "reserved":
      return strings.signupReserved;
    default:
      return strings.signupInvalid;
  }
}

export function SignupPage() {
  const navigate = useNavigate();
  const [domains, setDomains] = useState<string[] | null>(null);
  const [domain, setDomain] = useState("");
  const [step, setStep] = useState<Step>("address");

  const [localpart, setLocalpart] = useState("");
  const [recovery, setRecovery] = useState("");
  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);

  const [avail, setAvail] = useState<{ reason: string } | "checking" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const address = useMemo(
    () => (localpart.trim() === "" ? "" : `${localpart.trim().toLowerCase()}@${domain}`),
    [localpart, domain],
  );

  // Load the configured personal domains once; empty → signup is off.
  useEffect(() => {
    let live = true;
    void signupDomains().then((d) => {
      if (!live) return;
      setDomains(d);
      if (d[0] !== undefined) setDomain(d[0]);
    });
    return () => {
      live = false;
    };
  }, []);

  // Debounced availability check as the user types the local part.
  const debounce = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => {
    if (domain === "" || localpart.trim() === "") {
      setAvail(null);
      return;
    }
    setAvail("checking");
    clearTimeout(debounce.current);
    const addr = `${localpart.trim().toLowerCase()}@${domain}`;
    debounce.current = setTimeout(() => {
      void signupAvailable(addr)
        .then((r) => setAvail({ reason: r.reason }))
        .catch(() => setAvail(null));
    }, 350);
    return () => clearTimeout(debounce.current);
  }, [localpart, domain]);

  const canSend =
    !busy &&
    address !== "" &&
    recovery.trim() !== "" &&
    typeof avail === "object" &&
    avail?.reason === "ok";

  async function sendCode(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await signupBegin(address, recovery.trim());
      setStep("verify");
    } catch (e) {
      setError(e instanceof SignupError && e.message === "network"
        ? strings.errorNetwork
        : strings.signupBeginError);
    } finally {
      setBusy(false);
    }
  }

  async function create(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await signupVerify(address, code.trim(), password);
      setStep("done");
    } catch (e) {
      setError(e instanceof SignupError && e.message === "network"
        ? strings.errorNetwork
        : strings.signupVerifyError);
    } finally {
      setBusy(false);
    }
  }

  async function resend() {
    setError(null);
    try {
      await signupBegin(address, recovery.trim());
    } catch {
      setError(strings.signupBeginError);
    }
  }

  const signupOff = domains !== null && domains.length === 0;

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
        {domains === null ? (
          <Spinner size={24} />
        ) : signupOff ? (
          <div className={login.form}>
            <div className={login.formHead}>
              <h2 className={login.heading}>{strings.signupHeading}</h2>
              <p className={login.subtitle}>{strings.signupUnavailable}</p>
            </div>
            <Link to="/login" className={styles.loginLink}>
              {strings.signupBackToLogin}
            </Link>
          </div>
        ) : step === "done" ? (
          <div className={login.form}>
            <div className={login.formHead}>
              <h2 className={login.heading}>{strings.signupDoneHeading}</h2>
              <p className={login.subtitle}>{strings.signupDoneBody(address)}</p>
            </div>
            <Button block onClick={() => navigate("/login")}>
              {strings.signupGoToLogin}
            </Button>
          </div>
        ) : step === "verify" ? (
          <form className={login.form} onSubmit={create}>
            <div className={login.formHead}>
              <h2 className={login.heading}>{strings.signupVerifyHeading}</h2>
              <p className={login.subtitle}>{strings.signupVerifySubtitle(recovery.trim())}</p>
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
              <span className={login.label}>{strings.signupPasswordLabel}</span>
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
              {busy ? <Spinner size={16} label={strings.signupCreating} /> : strings.signupCreate}
            </Button>
            <button type="button" className={login.linkButton} onClick={() => void resend()}>
              {strings.signupResend}
            </button>
          </form>
        ) : (
          <form className={login.form} onSubmit={sendCode}>
            <div className={login.formHead}>
              <h2 className={login.heading}>{strings.signupHeading}</h2>
              <p className={login.subtitle}>{strings.signupSubtitle}</p>
            </div>

            <label className={login.field}>
              <span className={login.label}>{strings.signupAddressLabel}</span>
              <div className={styles.addressRow}>
                <input
                  className={`${login.input} ${styles.localpart}`}
                  value={localpart}
                  onChange={(e) => setLocalpart(e.target.value)}
                  placeholder={strings.signupPickPlaceholder}
                  autoComplete="off"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
                  required
                  autoFocus
                />
                {domains.length > 1 ? (
                  <select
                    className={styles.domainSelect}
                    value={domain}
                    onChange={(e) => setDomain(e.target.value)}
                    aria-label={strings.signupAddressLabel}
                  >
                    {domains.map((d) => (
                      <option key={d} value={d}>
                        @{d}
                      </option>
                    ))}
                  </select>
                ) : (
                  <span className={styles.domainSuffix}>@{domain}</span>
                )}
              </div>
              {avail === "checking" && <span className={styles.hint}>{strings.signupChecking}</span>}
              {typeof avail === "object" && avail !== null && (
                <span className={avail.reason === "ok" ? styles.ok : styles.bad}>
                  {reasonText(avail.reason)}
                </span>
              )}
            </label>

            <label className={login.field}>
              <span className={login.label}>{strings.signupRecoveryLabel}</span>
              <input
                className={login.input}
                type="email"
                autoComplete="email"
                value={recovery}
                onChange={(e) => setRecovery(e.target.value)}
                required
              />
              <span className={styles.hint}>{strings.signupRecoveryHint}</span>
            </label>

            {error !== null && (
              <p className={login.error} role="alert">
                {error}
              </p>
            )}

            <Button type="submit" block disabled={!canSend}>
              {busy ? <Spinner size={16} label={strings.signupSending} /> : strings.signupSendCode}
            </Button>

            <p className={styles.haveAccount}>
              {strings.signupHaveAccount}{" "}
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
