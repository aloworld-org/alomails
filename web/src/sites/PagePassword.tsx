// Who can open one page (S2.06b): the control that puts a page behind a
// password, changes that password, and takes it off again.
//
// Three decisions this screen is built on.
//
// **It says what the internet can see, not what a setting is called.** The
// state line is "anyone on the internet can open this page" or "only people
// with the password can" — the reader is deciding who reads their work, not
// toggling a field. What the visitor meets is spelled out here too, because
// the unlock screen deliberately shows nothing of the page (not even its
// title) and an owner who expects to see the page's name would otherwise
// think it broke.
//
// **The password goes in and never comes out.** No read on the server answers
// it, so this screen never renders a stored value: the field is always empty,
// and a forgotten password is replaced rather than recovered. The one help it
// gives is a show/hide toggle, so a typo can be seen before it is saved
// instead of after a visitor cannot get in.
//
// **Taking the password off is the one gesture that asks twice.** Setting and
// changing are reversible in a click; removing it is a disclosure — the page
// is public the moment it lands — and disclosure cannot be undone. So it arms
// first, exactly as taking a live site off the air does.
import { useCallback, useEffect, useRef, useState } from "react";
import { Eye, EyeOff, KeyRound, Lock, Unlock, X } from "lucide-react";

import { Button, IconButton, Input, Modal, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import type { SitePageProtection } from "./types";

/** How the day a password was set is named: a date a person recognises, in
 *  their own locale. The hour it happened is nobody's decision to review. */
const day = new Intl.DateTimeFormat(undefined, { dateStyle: "long" });

/** The date to show for a protection, or `null` when the server sent none —
 *  an unreadable timestamp drops the clause rather than printing "Invalid
 *  Date" next to a security state. */
function setOn(protection: SitePageProtection): string | null {
  const stamp = protection.updatedAt ?? protection.createdAt;
  if (stamp === null) return null;
  const moment = new Date(stamp);
  return Number.isNaN(moment.getTime()) ? null : day.format(moment);
}

export function PagePassword({
  siteId,
  pageId,
  multilingual = false,
  compact = false,
  navigation = false,
  onChange,
}: {
  siteId: string;
  pageId: string;
  /** Whether the site publishes in more than one language — then the panel
   *  says out loud that a password holds for all of them, because the reader
   *  is looking at one language's tab while deciding. */
  multilingual?: boolean;
  /** Uses the editor toolbar's surrounding surface instead of drawing a
   * second settings card around the same control. */
  compact?: boolean;
  /** Keeps the access state loaded while presenting its editor as a page-level
   * navigation action and focused dialog. */
  navigation?: boolean;
  /** Called with the page's protection state whenever it is known or changes,
   *  so the editor around this panel can tell the truth about its preview. */
  onChange?: (isProtected: boolean) => void;
}) {
  const api = useSitesApi();
  const [protection, setProtection] = useState<SitePageProtection | null>(null);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState(false);
  const [password, setPassword] = useState("");
  const [visible, setVisible] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<"saved" | "removed" | null>(null);
  const [confirmingRemove, setConfirmingRemove] = useState(false);
  const [accessOpen, setAccessOpen] = useState(false);

  // Held in a ref rather than watched as a dependency, for the reason
  // SchedulePublish holds its callback that way: an inline arrow from the
  // editor changes identity on every one of its renders, and the load effect
  // keyed on it would re-fetch forever.
  const announce = useRef(onChange);
  useEffect(() => {
    announce.current = onChange;
  }, [onChange]);

  const load = useCallback(async () => {
    try {
      const answer = await api.pagePassword(siteId, pageId);
      setProtection(answer);
      announce.current?.(answer.protected);
      setError(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesPagePasswordLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [announce, api, siteId, pageId]);

  useEffect(() => {
    void load();
  }, [load]);

  function open() {
    setPassword("");
    setVisible(false);
    setOutcome(null);
    setError(null);
    setConfirmingRemove(false);
    setEditing(true);
  }

  function closeNavigation() {
    setAccessOpen(false);
    setEditing(false);
    setPassword("");
    setVisible(false);
    setError(null);
    setOutcome(null);
    setConfirmingRemove(false);
  }

  async function save() {
    // The only rule this screen owns: an empty field is not a password. Every
    // other rule (length, spaces) is the store's, and its sentence is shown
    // verbatim rather than guessed at a second time here.
    if (password === "") {
      setError(strings.sitesPagePasswordMissing);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const answer = await api.setPagePassword(siteId, pageId, password);
      setProtection(answer);
      announce.current?.(answer.protected);
      setPassword("");
      setVisible(false);
      setEditing(false);
      setOutcome("saved");
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesPagePasswordSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!confirmingRemove) {
      setConfirmingRemove(true);
      setOutcome(null);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const answer = await api.removePagePassword(siteId, pageId);
      setProtection(answer);
      announce.current?.(answer.protected);
      setEditing(false);
      setOutcome("removed");
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesPagePasswordRemoveFailed));
    } finally {
      setBusy(false);
      setConfirmingRemove(false);
    }
  }

  const isProtected = protection?.protected === true;
  const since = protection === null ? null : setOn(protection);

  const panel = (
    <section
      className={`flex flex-col gap-3 ${
        navigation
          ? ""
          : compact
          ? "border-t border-subtle px-4 py-3 sm:px-5"
          : "rounded-2xl border border-subtle bg-surface p-5 shadow-sm"
      }`}
      aria-labelledby="page-password-title"
    >
      <div className="flex flex-wrap items-start gap-3">
        <span
          className={`inline-flex size-10 shrink-0 items-center justify-center rounded-xl ${
            isProtected
              ? "bg-accent-soft text-accent"
              : "bg-raised text-tertiary"
          }`}
          aria-hidden="true"
        >
          {isProtected ? <Lock size={18} /> : <Unlock size={18} />}
        </span>
        <div className="min-w-0 flex-1">
          <h2
            id="page-password-title"
            className="text-sm font-semibold text-primary"
          >
            {strings.sitesPagePasswordTitle}
          </h2>
          {loading ? (
            <p className="mt-1 flex items-center gap-2 text-sm text-secondary">
              <Spinner size={14} /> {strings.sitesPagePasswordLoading}
            </p>
          ) : protection === null ? (
            // The state could not be read. Saying "anyone can open this page"
            // here would be a guess about who can read the owner's work, and
            // a wrong guess in the reassuring direction; the error below says
            // what happened instead.
            <p className="mt-1 text-sm text-secondary">
              {strings.sitesPagePasswordUnknown}
            </p>
          ) : (
            <>
              <p className="mt-1 text-sm text-primary" role="status">
                {isProtected
                  ? since === null
                    ? strings.sitesPagePasswordProtectedUndated
                    : strings.sitesPagePasswordProtected(since)
                  : strings.sitesPagePasswordPublic}
              </p>
              <p
                className={
                  compact
                    ? "sr-only"
                    : "mt-1 max-w-2xl text-sm leading-6 text-secondary"
                }
              >
                {isProtected
                  ? strings.sitesPagePasswordProtectedHint
                  : strings.sitesPagePasswordPublicHint}
              </p>
              {isProtected && multilingual && (
                <p
                  className={
                    compact ? "sr-only" : "mt-1 text-sm text-secondary"
                  }
                >
                  {strings.sitesPagePasswordEveryLanguage}
                </p>
              )}
              {outcome !== null && (
                <p
                  className="mt-1 text-sm font-medium text-primary"
                  role="status"
                >
                  {outcome === "saved"
                    ? strings.sitesPagePasswordSaved
                    : strings.sitesPagePasswordRemoved}
                </p>
              )}
            </>
          )}
        </div>
        <div className="flex w-full items-center justify-end gap-2 sm:ml-auto sm:w-auto">
          {isProtected && (
            <Button
              variant={confirmingRemove ? "danger" : "ghost"}
              size="sm"
              icon={<Unlock size="var(--icon-size-inline)" />}
              disabled={busy}
              onClick={() => void remove()}
            >
              {confirmingRemove
                ? strings.sitesPagePasswordRemoveConfirm
                : strings.sitesPagePasswordRemove}
            </Button>
          )}
          {!editing && !loading && protection !== null && (
            <Button
              variant={isProtected ? "ghost" : "primary"}
              size="sm"
              icon={<KeyRound size="var(--icon-size-inline)" />}
              disabled={busy}
              onClick={open}
            >
              {isProtected
                ? strings.sitesPagePasswordChange
                : strings.sitesPagePasswordProtect}
            </Button>
          )}
        </div>
      </div>

      {editing && (
        <div className="flex flex-wrap items-end gap-3 rounded-xl bg-raised p-3">
          <label className="flex min-w-64 flex-1 flex-col gap-1.5 text-xs font-medium text-secondary">
            <span>{strings.sitesPagePasswordField}</span>
            <span className="flex items-center gap-2">
              <Input
                type={visible ? "text" : "password"}
                value={password}
                autoComplete="new-password"
                disabled={busy}
                onChange={(event) => setPassword(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void save();
                  }
                }}
              />
              <Button
                variant="ghost"
                size="sm"
                icon={
                  visible ? (
                    <EyeOff size="var(--icon-size-inline)" />
                  ) : (
                    <Eye size="var(--icon-size-inline)" />
                  )
                }
                aria-pressed={visible}
                onClick={() => setVisible(!visible)}
              >
                {visible
                  ? strings.sitesPagePasswordHide
                  : strings.sitesPagePasswordShow}
              </Button>
            </span>
          </label>
          <p className="flex min-w-48 flex-1 flex-col gap-1 text-xs leading-5 text-secondary">
            <span>{strings.sitesPagePasswordFieldHint}</span>
            <span>{strings.sitesPagePasswordEffective}</span>
          </p>
          <div className="ml-auto flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              disabled={busy}
              onClick={() => {
                setEditing(false);
                setPassword("");
                setVisible(false);
                setError(null);
              }}
            >
              {strings.cancel}
            </Button>
            <Button size="sm" disabled={busy} onClick={() => void save()}>
              {busy
                ? strings.sitesPagePasswordSaving
                : isProtected
                  ? strings.sitesPagePasswordChange
                  : strings.sitesPagePasswordProtect}
            </Button>
          </div>
        </div>
      )}

      {error !== null && (
        <p
          className="rounded-lg bg-danger-tint px-3 py-2 text-sm text-primary"
          role="alert"
        >
          {error}
        </p>
      )}
    </section>
  );

  if (!navigation) return panel;

  return (
    <>
      <Button
        variant="ghost"
        size="sm"
        icon={
          isProtected ? (
            <Lock size="var(--icon-size-inline)" />
          ) : (
            <Unlock size="var(--icon-size-inline)" />
          )
        }
        className={
          isProtected
            ? "!border-accent/20 !bg-accent-soft !text-accent"
            : undefined
        }
        aria-haspopup="dialog"
        aria-expanded={accessOpen}
        onClick={() => setAccessOpen(true)}
      >
        {strings.sitesPageAccess}
      </Button>
      {accessOpen && (
        <Modal
          title={strings.sitesPageAccess}
          wide
          icon={
            isProtected ? (
              <Lock size="var(--icon-size-control)" />
            ) : (
              <Unlock size="var(--icon-size-control)" />
            )
          }
          actions={
            <IconButton
              label={strings.close}
              icon={<X size="var(--icon-size-control)" />}
              onClick={closeNavigation}
            />
          }
          onClose={closeNavigation}
        >
          {panel}
        </Modal>
      )}
    </>
  );
}
