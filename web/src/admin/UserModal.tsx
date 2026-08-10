// Create a user, or manage an existing one (reset password, aliases, delete).
// Admin-gated on the server; the calling page also hides self-destructive
// actions for the signed-in admin.
import { useState } from "react";
import type { FormEvent } from "react";
import { X } from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner, useDialogs } from "../ds";
import { useJmapClient } from "../jmap";
import type { AdminUser } from "../jmap";
import styles from "./admin.module.css";

interface UserModalProps {
  user?: AdminUser;
  isSelf: boolean;
  onClose: () => void;
  onChanged: () => void;
}

export function UserModal({ user, isSelf, onClose, onChanged }: UserModalProps) {
  const { confirm } = useDialogs();
  const client = useJmapClient();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [aliases, setAliases] = useState<string[]>(user?.aliases ?? []);
  const [accountant, setAccountant] = useState(user?.roles.includes("accountant") ?? false);
  const [aliasDraft, setAliasDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  async function create(e: FormEvent) {
    e.preventDefault();
    if (!email.includes("@") || password.length < 8) {
      setError(strings.userInvalid);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await client.createUser(email.trim(), password);
      onChanged();
    } catch {
      setError(strings.userCreateError);
      setBusy(false);
    }
  }

  async function reset() {
    if (user === undefined || password.length < 8 || busy) return;
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      await client.resetPassword(user.id, password);
      setPassword("");
      setNote(strings.userResetDone);
    } catch {
      setError(strings.userActionError);
    } finally {
      setBusy(false);
    }
  }

  /** Grant or revoke the accountant role. Optimistic, and put back if the
   * server refuses — an access control that lies about its state is worse than
   * one that is slow. */
  async function toggleAccountant() {
    if (user === undefined || busy) return;
    const next = !accountant;
    setAccountant(next);
    setBusy(true);
    setError(null);
    try {
      await client.setUserRole(user.id, "accountant", next);
      onChanged();
    } catch {
      setAccountant(!next);
      setError(strings.userActionError);
    } finally {
      setBusy(false);
    }
  }

  async function addAlias() {
    const a = aliasDraft.trim();
    if (user === undefined || !a.includes("@") || aliases.includes(a)) return;
    setBusy(true);
    try {
      await client.addAlias(user.id, a);
      setAliases([...aliases, a]);
      setAliasDraft("");
    } catch {
      setError(strings.userActionError);
    } finally {
      setBusy(false);
    }
  }

  async function removeAlias(a: string) {
    setBusy(true);
    try {
      await client.removeAlias(a);
      setAliases(aliases.filter((x) => x !== a));
    } catch {
      setError(strings.userActionError);
    } finally {
      setBusy(false);
    }
  }

  async function del() {
    if (user === undefined || !(await confirm({ message: strings.userDeleteConfirm(user.email), danger: true }))) return;
    setBusy(true);
    try {
      await client.deleteUser(user.id);
      onChanged();
    } catch {
      setError(strings.userActionError);
      setBusy(false);
    }
  }

  if (user === undefined) {
    return (
      <div className={styles.overlay} onMouseDown={onClose}>
        <div
          className={styles.modal}
          role="dialog"
          aria-modal="true"
          aria-label={strings.adminAddUser}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <form onSubmit={create}>
            <div className={styles.modalHead}>
              <h2>{strings.adminAddUser}</h2>
              <button type="button" className={styles.iconBtn} onClick={onClose} aria-label={strings.userClose}>
                <X size={18} />
              </button>
            </div>
            <div className={styles.modalBody}>
              <label className={styles.field}>
                <span className={styles.label}>{strings.userEmail}</span>
                <input
                  className={styles.input}
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="name@namel3ss.com"
                  autoFocus
                />
              </label>
              <label className={styles.field}>
                <span className={styles.label}>{strings.userPassword}</span>
                <input
                  className={styles.input}
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={strings.userPasswordHint}
                />
              </label>
              {error !== null && (
                <p className={styles.error} role="alert">
                  {error}
                </p>
              )}
            </div>
            <div className={styles.modalFoot}>
              <div className={styles.footSpacer} />
              <button type="button" className={styles.textBtn} onClick={onClose}>
                {strings.providerCancel}
              </button>
              <Button type="submit" disabled={busy}>
                {busy ? <Spinner size={16} /> : strings.userCreate}
              </Button>
            </div>
          </form>
        </div>
      </div>
    );
  }

  return (
    <div className={styles.overlay} onMouseDown={onClose}>
      <div
        className={styles.modal}
        role="dialog"
        aria-modal="true"
        aria-label={user.email}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className={styles.modalHead}>
          <h2>{user.email}</h2>
          <button type="button" className={styles.iconBtn} onClick={onClose} aria-label={strings.userClose}>
            <X size={18} />
          </button>
        </div>
        <div className={styles.modalBody}>
          <div className={styles.field}>
            <span className={styles.label}>{strings.userNewPassword}</span>
            <div className={styles.keyRow}>
              <input
                className={styles.input}
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder={strings.userPasswordHint}
              />
              <button
                type="button"
                className={styles.ghost}
                onClick={() => void reset()}
                disabled={busy || password.length < 8}
              >
                {strings.userReset}
              </button>
            </div>
            {note !== null && <span className={styles.hintOk}>{note}</span>}
          </div>

          {/* Scoped roles (ADR 0035, B4.12). A named checkbox with the whole
              rule written beside it, not a bare switch: an access grant is the
              one control where "what does this do?" must be answerable without
              trying it. */}
          <div className={styles.field}>
            <span className={styles.label}>{strings.userRoles}</span>
            <div className={styles.keyRow}>
              <label className={styles.toggle} aria-label={strings.userAccountantRole}>
                <input
                  type="checkbox"
                  checked={accountant}
                  disabled={busy}
                  onChange={() => void toggleAccountant()}
                />
                <span className={styles.track} />
              </label>
              <span>{strings.userAccountantRole}</span>
            </div>
            <span className={styles.hint}>{strings.userAccountantHint}</span>
          </div>

          <div className={styles.field}>
            <span className={styles.label}>{strings.userAliases}</span>
            <div className={styles.chips}>
              {aliases.map((a) => (
                <span key={a} className={styles.chip}>
                  <span className={styles.chipLabel}>{a}</span>
                  <button
                    type="button"
                    className={styles.chipX}
                    onClick={() => void removeAlias(a)}
                    aria-label={strings.providerRemoveModel(a)}
                  >
                    <X size={12} />
                  </button>
                </span>
              ))}
              <input
                className={styles.chipInput}
                value={aliasDraft}
                onChange={(e) => setAliasDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === ",") {
                    e.preventDefault();
                    void addAlias();
                  }
                }}
                placeholder={strings.userAliasPlaceholder}
              />
              <button type="button" className={styles.addChip} onClick={() => void addAlias()}>
                {strings.providerAddModel}
              </button>
            </div>
          </div>

          {error !== null && (
            <p className={styles.error} role="alert">
              {error}
            </p>
          )}
        </div>
        <div className={styles.modalFoot}>
          <button type="button" className={styles.dangerBtn} onClick={() => void del()} disabled={busy || isSelf}>
            {strings.userDelete}
          </button>
          <div className={styles.footSpacer} />
          <button type="button" className={styles.primary} onClick={onClose}>
            {strings.userClose}
          </button>
        </div>
      </div>
    </div>
  );
}
