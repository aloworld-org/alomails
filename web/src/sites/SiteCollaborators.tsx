import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Copy, Link2, RotateCcw, Trash2, UserPlus, Users } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import type { SiteCollaborator } from "./types";

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
    <section
      className="grid gap-5 rounded-2xl border border-subtle bg-surface px-5 py-5 shadow-sm sm:px-6 lg:grid-cols-[minmax(0,1fr)_minmax(22rem,1fr)] lg:items-start"
      aria-labelledby="site-collaborators-title"
    >
      <div className="flex items-start gap-3">
        <span
          className="inline-flex size-11 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent [&>svg]:size-5"
          aria-hidden="true"
        >
          <Users />
        </span>
        <div>
          <h2
            id="site-collaborators-title"
            className="m-0 text-base font-semibold text-text-primary"
          >
            {strings.sitesCollaborators}
          </h2>
          <p className="mb-0 mt-1 max-w-xl text-sm leading-5 text-text-secondary">
            {strings.sitesCollaboratorsHint}
          </p>
        </div>
      </div>

      <form
        className="flex min-w-0 flex-col gap-2 sm:flex-row sm:items-end"
        onSubmit={submit}
      >
        <label className="flex min-w-0 flex-1 flex-col gap-1.5 text-xs font-medium text-text-secondary">
          <span>{strings.sitesCollaboratorEmail}</span>
          <input
            className="min-h-11 w-full rounded-xl border border-default bg-surface px-3.5 text-sm text-text-primary outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/20 disabled:cursor-not-allowed disabled:opacity-60"
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
        <div
          className="flex items-center gap-2 text-sm text-text-tertiary lg:col-span-2"
          role="status"
        >
          <Spinner size={16} />
          {strings.sitesCollaboratorsLoading}
        </div>
      )}
      {error !== null && (
        <p
          className="m-0 rounded-xl border border-danger bg-danger-tint px-4 py-3 text-sm text-text-primary lg:col-span-2"
          role="alert"
        >
          {error}
        </p>
      )}
      {notice !== null && (
        <p
          className="m-0 rounded-xl border-l-2 border-success bg-surface-raised px-4 py-3 text-sm text-success lg:col-span-2"
          role="status"
        >
          {notice}
        </p>
      )}
      {revoked !== null && (
        <div
          className="flex flex-wrap items-center justify-between gap-2 rounded-xl bg-surface-raised px-4 py-3 text-sm text-text-secondary lg:col-span-2"
          role="status"
        >
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
        <p className="m-0 rounded-xl bg-surface-raised px-4 py-3 text-sm text-text-secondary lg:col-span-2">
          {strings.sitesNoCollaborators}
        </p>
      )}
      <div className="flex flex-col gap-1 lg:col-span-2">
        {collaborators.map((collaborator) => {
          const inviteLink = inviteLinks[collaborator.id];
          return (
            <div
              className="flex min-h-12 flex-wrap items-center gap-3 rounded-xl px-2 py-1 hover:bg-surface-raised"
              key={collaborator.id}
            >
              <span
                className="inline-flex size-10 shrink-0 items-center justify-center rounded-full bg-accent-soft text-sm font-semibold text-text-primary"
                aria-hidden="true"
              >
                {collaborator.email.slice(0, 1).toUpperCase()}
              </span>
              <span className="flex min-w-0 flex-1 flex-col gap-1">
                <strong className="truncate text-sm font-medium text-text-primary">
                  {collaborator.email}
                </strong>
                <span className="text-xs text-text-tertiary">
                  {collaborator.status === "pending"
                    ? strings.sitesCollaboratorPending
                    : strings.sitesCollaboratorActive}
                </span>
              </span>
              <span className="flex w-full flex-wrap items-center justify-end gap-2 sm:w-auto">
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
