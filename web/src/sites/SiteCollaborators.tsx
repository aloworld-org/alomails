import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Copy, Link2, RotateCcw, Trash2, UserPlus, Users } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import type { SiteCollaborator } from "./types";
import styles from "./SitesModule.module.css";

export function SiteCollaborators({ siteId }: { siteId: string }) {
  const api = useSitesApi();
  const [collaborators, setCollaborators] = useState<SiteCollaborator[]>([]);
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [inviteLinks, setInviteLinks] = useState<Record<string, string>>({});
  const [revoked, setRevoked] = useState<{ email: string } | null>(null);

  const load = useCallback(async () => {
    try {
      setCollaborators(await api.collaborators(siteId));
      setError(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCollaboratorsLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function invite(address: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const result = await api.inviteCollaborator(siteId, address);
      if (result.inviteUrl !== null) {
        setInviteLinks((current) => ({
          ...current,
          [result.collaborator.id]: result.inviteUrl ?? "",
        }));
        setNotice(strings.sitesCollaboratorLinkReady(result.collaborator.email));
      } else {
        setNotice(strings.sitesCollaboratorAdded(result.collaborator.email));
      }
      setEmail("");
      setRevoked(null);
      await load();
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCollaboratorInviteFailed));
    } finally {
      setBusy(false);
    }
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    const address = email.trim();
    if (address !== "") void invite(address);
  }

  async function copyInvite(collaborator: SiteCollaborator) {
    const link = inviteLinks[collaborator.id];
    if (link === undefined) return;
    try {
      await navigator.clipboard.writeText(link);
      setNotice(strings.sitesCollaboratorLinkCopied);
      setError(null);
    } catch {
      setError(strings.sitesCollaboratorCopyFailed);
    }
  }

  async function revoke(collaborator: SiteCollaborator) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await api.revokeCollaborator(siteId, collaborator.id);
      setRevoked({ email: collaborator.email });
      setInviteLinks((current) => {
        const next = { ...current };
        delete next[collaborator.id];
        return next;
      });
      await load();
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCollaboratorRevokeFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={styles.collaboratorPanel} aria-labelledby="site-collaborators-title">
      <div className={styles.collaboratorIntro}>
        <span className={styles.collaboratorIcon} aria-hidden="true">
          <Users />
        </span>
        <div>
          <h2 id="site-collaborators-title" className={styles.collaboratorTitle}>
            {strings.sitesCollaborators}
          </h2>
          <p className={styles.collaboratorHint}>{strings.sitesCollaboratorsHint}</p>
        </div>
      </div>

      <form className={styles.collaboratorInvite} onSubmit={submit}>
        <label className={styles.collaboratorEmail}>
          <span>{strings.sitesCollaboratorEmail}</span>
          <input
            className={styles.input}
            type="email"
            autoComplete="email"
            value={email}
            placeholder={strings.sitesCollaboratorEmailPlaceholder}
            disabled={busy}
            onChange={(event) => setEmail(event.target.value)}
          />
        </label>
        <Button
          type="submit"
          size="sm"
          icon={<UserPlus size="var(--icon-size-inline)" />}
          disabled={busy || email.trim() === ""}
        >
          {strings.sitesInviteCollaborator}
        </Button>
      </form>

      {loading && (
        <div className={styles.collaboratorStatus} role="status">
          <Spinner size={16} />
          {strings.sitesCollaboratorsLoading}
        </div>
      )}
      {error !== null && (
        <p className={styles.publishError} role="alert">
          {error}
        </p>
      )}
      {notice !== null && (
        <p className={styles.collaboratorNotice} role="status">
          {notice}
        </p>
      )}
      {revoked !== null && (
        <div className={styles.collaboratorUndo} role="status">
          <span>{strings.sitesCollaboratorRevoked(revoked.email)}</span>
          <Button
            variant="ghost"
            size="sm"
            icon={<RotateCcw size="var(--icon-size-inline)" />}
            disabled={busy}
            onClick={() => void invite(revoked.email)}
          >
            {strings.sitesUndoCollaboratorRevoke}
          </Button>
        </div>
      )}

      {!loading && collaborators.length === 0 && (
        <p className={styles.collaboratorEmpty}>{strings.sitesNoCollaborators}</p>
      )}
      <div className={styles.collaboratorRows}>
        {collaborators.map((collaborator) => {
          const inviteLink = inviteLinks[collaborator.id];
          return (
            <div className={styles.collaboratorRow} key={collaborator.id}>
              <span className={styles.collaboratorAvatar} aria-hidden="true">
                {collaborator.email.slice(0, 1).toUpperCase()}
              </span>
              <span className={styles.collaboratorIdentity}>
                <strong>{collaborator.email}</strong>
                <span>
                  {collaborator.status === "pending"
                    ? strings.sitesCollaboratorPending
                    : strings.sitesCollaboratorActive}
                </span>
              </span>
              <span className={styles.collaboratorActions}>
                {collaborator.status === "pending" && inviteLink === undefined && (
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<Link2 size="var(--icon-size-inline)" />}
                    disabled={busy}
                    onClick={() => void invite(collaborator.email)}
                  >
                    {strings.sitesRefreshCollaboratorLink}
                  </Button>
                )}
                {inviteLink !== undefined && (
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<Copy size="var(--icon-size-inline)" />}
                    disabled={busy}
                    onClick={() => void copyInvite(collaborator)}
                  >
                    {strings.sitesCopyCollaboratorLink}
                  </Button>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Trash2 size="var(--icon-size-inline)" />}
                  disabled={busy}
                  onClick={() => void revoke(collaborator)}
                >
                  {strings.sitesRevokeCollaborator}
                </Button>
              </span>
            </div>
          );
        })}
      </div>
    </section>
  );
}
