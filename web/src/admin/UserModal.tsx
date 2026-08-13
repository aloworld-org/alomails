// Create a user, or manage an existing one (reset password, aliases, delete).
// Admin-gated on the server; the calling page also hides self-destructive
// actions for the signed-in admin.
import { useEffect, useId, useState } from "react";
import type { FormEvent } from "react";
import { X } from "lucide-react";

import { strings } from "../i18n";
import {
  Button,
  Chip,
  Field,
  IconButton,
  Input,
  Modal,
  Spinner,
  Toggle,
  useDialogs,
} from "../ds";
import { useJmapClient } from "../jmap";
import type { AdminUser, AppModuleId, UserModuleAccess } from "../jmap";
import styles from "./admin.module.css";

/** The label each switchable module shows, in the console's own order.
 *
 * A record rather than a lookup function, so adding a module to the union and
 * forgetting its label is a type error here rather than a blank checkbox in
 * production. */
const MODULE_LABEL: Record<AppModuleId, () => string> = {
  agenda: () => strings.moduleAgenda,
  billing: () => strings.moduleBilling,
  chat: () => strings.moduleChat,
  crm: () => strings.moduleCrm,
  drive: () => strings.moduleDrive,
  finance: () => strings.moduleFinance,
  hr: () => strings.moduleHr,
  insights: () => strings.moduleInsights,
  inventory: () => strings.moduleInventory,
  meet: () => strings.moduleMeet,
  projects: () => strings.moduleProjects,
  sites: () => strings.moduleSites,
  tasks: () => strings.moduleTasks,
};

interface UserModalProps {
  user?: AdminUser;
  isSelf: boolean;
  onClose: () => void;
  /** Something changed and this dialog's work is finished — reload and close. */
  onChanged: () => void;
  /** Something changed and the dialog must stay open. */
  onSaved: () => void;
}

export function UserModal({
  user,
  isSelf,
  onClose,
  onChanged,
  onSaved,
}: UserModalProps) {
  const { confirm } = useDialogs();
  const client = useJmapClient();
  const formId = useId();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [aliases, setAliases] = useState<string[]>(user?.aliases ?? []);
  const [accountant, setAccountant] = useState(
    user?.roles.includes("accountant") ?? false,
  );
  const [aliasDraft, setAliasDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  // The per-user app switches (migration 0208). `null` until they load, which
  // is drawn as a spinner rather than as thirteen unchecked boxes — an access
  // screen that renders "no access to anything" while it is still asking is
  // the one wrong answer here.
  const [modules, setModules] = useState<UserModuleAccess[] | null>(null);
  // The setup link, once an invitation has been created. Held rather than
  // shown-and-forgotten: it is minted once, the server keeps only its hash,
  // and closing the dialog without copying it means sending another.
  const [inviteUrl, setInviteUrl] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const userId = user?.id;
  useEffect(() => {
    if (userId === undefined) return;
    let live = true;
    void (async () => {
      try {
        const list = await client.userModules(userId);
        if (live) setModules(list);
      } catch {
        // Leave the section as a spinner rather than showing switches that
        // might not be this person's. The rest of the modal still works.
      }
    })();
    return () => {
      live = false;
    };
  }, [client, userId]);

  /** Switch one app on or off. Optimistic, and put back if the server refuses
   * — an access control that lies about its state is worse than one that is
   * slow, the same rule the role toggle above follows. */
  async function toggleModule(id: AppModuleId, allowed: boolean) {
    if (user === undefined || busy) return;
    setModules(
      (prev) => prev?.map((m) => (m.id === id ? { ...m, allowed } : m)) ?? prev,
    );
    setBusy(true);
    setError(null);
    try {
      await client.setUserModule(user.id, id, allowed);
      // Deliberately no `onChanged()`. That closes the modal, which is right
      // after creating or deleting somebody and wrong here: an admin setting
      // up a new colleague switches several apps in a row, and a dialog that
      // shut after the first one would make thirteen checkboxes unusable.
      // Nothing in the user list shows app access, so there is nothing to
      // refresh either.
    } catch {
      setModules(
        (prev) =>
          prev?.map((m) => (m.id === id ? { ...m, allowed: !allowed } : m)) ??
          prev,
      );
      setError(strings.userActionError);
    } finally {
      setBusy(false);
    }
  }

  /** Creates the colleague with no password and shows their setup link.
   *
   * The better of the two paths, and the default the copy points at: the
   * person chooses their own password and names their own recovery address,
   * and the admin learns neither. Setting a password here instead is still
   * offered, for a shared mailbox nobody signs into or somebody with no second
   * address to be reached at. */
  async function invite() {
    if (!email.includes("@")) {
      setError(strings.userInvalid);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const created = await client.inviteUser(email);
      setInviteUrl(created.inviteUrl);
      // Not `onChanged()`: that closes the dialog, and this link cannot be
      // shown again — the server keeps only its hash.
      onSaved();
    } catch {
      setError(strings.userActionError);
    } finally {
      setBusy(false);
    }
  }

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
    if (
      user === undefined ||
      !(await confirm({
        message: strings.userDeleteConfirm(user.email),
        danger: true,
      }))
    )
      return;
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
      <Modal
        title={strings.adminAddUser}
        onClose={onClose}
        actions={
          <IconButton
            label={strings.userClose}
            icon={<X size={18} />}
            onClick={onClose}
          />
        }
        footer={
          <>
            <div className={styles.footSpacer} />
            <Button variant="ghost" onClick={onClose}>
              {strings.providerCancel}
            </Button>
            {/* Two ways to make an account, and the invitation is the one
                the copy recommends: it is the only one where the admin does
                not end up knowing somebody else's password. */}
            <Button
              variant="ghost"
              onClick={() => void invite()}
              disabled={busy}
            >
              {strings.userInvite}
            </Button>
            <Button type="submit" form={formId} disabled={busy}>
              {busy ? <Spinner size={16} /> : strings.userCreate}
            </Button>
          </>
        }
      >
        {/* Body and footer are siblings inside the dialog, so Create is tied to
            the form by id — which also keeps Enter in either field working. */}
        <form id={formId} className={styles.userForm} onSubmit={create}>
          <Field label={strings.userEmail}>
            {(control) => (
              <Input
                {...control}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="name@namel3ss.com"
                autoFocus
              />
            )}
          </Field>
          <Field label={strings.userPassword}>
            {(control) => (
              <Input
                {...control}
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder={strings.userPasswordHint}
              />
            )}
          </Field>
          {inviteUrl !== null && (
            <Field
              label={strings.userInviteReady}
              hint={strings.userInviteHint}
            >
              {(control) => (
                <div className={styles.keyRow}>
                  <Input
                    {...control}
                    className={styles.keyRowGrow}
                    readOnly
                    value={inviteUrl}
                    onFocus={(e) => e.currentTarget.select()}
                  />
                  <Button
                    variant="ghost"
                    onClick={() => {
                      void navigator.clipboard.writeText(inviteUrl);
                      setCopied(true);
                    }}
                  >
                    {copied ? strings.userInviteCopied : strings.userInviteCopy}
                  </Button>
                </div>
              )}
            </Field>
          )}
          {error !== null && (
            <p className={styles.error} role="alert">
              {error}
            </p>
          )}
        </form>
      </Modal>
    );
  }

  return (
    <Modal
      title={user.email}
      onClose={onClose}
      actions={
        <IconButton
          label={strings.userClose}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          <button
            type="button"
            className={styles.dangerBtn}
            onClick={() => void del()}
            disabled={busy || isSelf}
          >
            {strings.userDelete}
          </button>
          <div className={styles.footSpacer} />
          <Button onClick={onClose}>{strings.userClose}</Button>
        </>
      }
    >
      <Field label={strings.userNewPassword}>
        {(control) => (
          <>
            <div className={styles.keyRow}>
              <Input
                {...control}
                className={styles.keyRowGrow}
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder={strings.userPasswordHint}
              />
              <Button
                variant="ghost"
                onClick={() => void reset()}
                disabled={busy || password.length < 8}
              >
                {strings.userReset}
              </Button>
            </div>
            {note !== null && <span className={styles.hintOk}>{note}</span>}
          </>
        )}
      </Field>

      {/* Scoped roles (ADR 0035, B4.12). A named switch with the whole rule
          written beside it, not a bare one: an access grant is the one control
          where "what does this do?" must be answerable without trying it. The
          label used to be an `aria-label` on a `<label>` whose visible text sat
          in a sibling `<span>` bound to nothing — so the words on screen and
          the words read out were two unrelated things. */}
      <div className={styles.block}>
        <span className={styles.label}>{strings.userRoles}</span>
        <Toggle
          checked={accountant}
          disabled={busy}
          onChange={() => void toggleAccountant()}
          label={strings.userAccountantRole}
          hint={strings.userAccountantHint}
        />
      </div>

      {/* The apps this person gets (migration 0208). Checked means they
          have it, which is the sentence an administrator thinks in — the
          server stores the complement, and neither side has to know.

          The rail hides what is switched off and the API refuses it, so
          this is a real decision rather than a tidy-up of somebody's
          sidebar. The hint says so, because "hidden" and "refused" are
          very different promises and an admin is entitled to know which
          one they are making. */}
      <div className={styles.block}>
        <span className={styles.label}>{strings.userApps}</span>
        {modules === null ? (
          <Spinner />
        ) : (
          <div className={styles.appGrid}>
            {modules.map((m) => (
              <Toggle
                key={m.id}
                checked={m.allowed}
                disabled={busy}
                onChange={() => void toggleModule(m.id, !m.allowed)}
                label={MODULE_LABEL[m.id]()}
              />
            ))}
          </div>
        )}
        <span className={styles.hint}>
          {isSelf ? strings.userAppsSelfHint : strings.userAppsHint}
        </span>
      </div>

      <div className={styles.block}>
        <span className={styles.label}>{strings.userAliases}</span>
        <div className={styles.chipRow}>
          {aliases.map((a) => (
            <Chip
              key={a}
              onRemove={() => void removeAlias(a)}
              removeLabel={strings.providerRemoveModel(a)}
            >
              {a}
            </Chip>
          ))}
          <input
            className={styles.chipInput}
            aria-label={strings.userAliasAdd}
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
          <button
            type="button"
            className={styles.addChip}
            onClick={() => void addAlias()}
          >
            {strings.providerAddModel}
          </button>
        </div>
      </div>

      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
