// A website's contact inbox: the queue and the selected message share one
// surface, like Mail, so reading and resolving a visitor request never
// navigates away or hides the next action in a menu.
//
// It is also where an enquiry becomes an opportunity (S2.10c). That handoff
// belongs here rather than on a separate screen because this is where a person
// is when they decide an enquiry is worth pursuing, and because everything the
// sales card needs — who wrote in, from which form, when — is already on this
// surface and must never be typed a second time.
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  Check,
  Download,
  Handshake,
  Inbox,
  RotateCcw,
  Unlink,
} from "lucide-react";
import { Link, useNavigate, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { saveTextFile } from "../platform/download";
import { SitesError, sitesMessage, useSitesApi } from "./api";
import { funnelMoney } from "./funnelReading";
import { HandoffDialog } from "./HandoffDialog";
import { EmptyState, ErrorBanner } from "./parts";
import type { SiteDetail, SiteLeadLink, SiteSubmission } from "./types";

const received = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

export function SubmissionsView() {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [submissions, setSubmissions] = useState<SiteSubmission[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The enquiries already handed to the sales board. A reader who may not see
  // CRM — a site editor, or a colleague it is switched off for — simply never
  // sees the handoff: the inbox is theirs and works unchanged.
  const [leads, setLeads] = useState<SiteLeadLink[]>([]);
  const [salesVisible, setSalesVisible] = useState(false);
  const [handingOff, setHandingOff] = useState<SiteSubmission | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [detail, rows] = await Promise.all([
        api.site(siteId),
        api.submissions(siteId),
      ]);
      setSite(detail);
      setSubmissions(rows);
      setSelectedId((current) =>
        current !== null && rows.some((row) => row.id === current)
          ? current
          : (rows[0]?.id ?? null),
      );
      setError(null);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesSubmissionsLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let current = true;
    void api
      .siteLeads(siteId)
      .then((links) => {
        if (!current) return;
        setLeads(links);
        setSalesVisible(true);
      })
      .catch((err: unknown) => {
        if (!current) return;
        // A `403` is the answer, not a failure: this reader does not get the
        // sales half of the screen. Anything else is worth saying out loud.
        setSalesVisible(false);
        if (!(err instanceof SitesError && err.status === 403)) {
          setError(sitesMessage(err, strings.sitesLeadsLoadFailed));
        }
      });
    return () => {
      current = false;
    };
  }, [api, siteId]);

  const selected = useMemo(
    () =>
      submissions.find((submission) => submission.id === selectedId) ?? null,
    [selectedId, submissions],
  );

  const leadBySubmission = useMemo(
    () => new Map(leads.map((link) => [link.submissionId, link])),
    [leads],
  );
  const selectedLead =
    selected === null ? undefined : leadBySubmission.get(selected.id);

  /** Unclaims the opportunity for this website. The opportunity itself stays
   *  on the board untouched — this undoes the link, not somebody's sale. */
  async function unlink(link: SiteLeadLink) {
    setBusyId(link.submissionId);
    setError(null);
    const previous = leads;
    setLeads((current) => current.filter((row) => row.id !== link.id));
    try {
      await api.deleteSiteLead(siteId, link.id);
    } catch (err) {
      setLeads(previous);
      setError(sitesMessage(err, strings.sitesUnlinkLeadFailed));
    } finally {
      setBusyId(null);
    }
  }

  async function setHandled(submission: SiteSubmission, handled: boolean) {
    setBusyId(submission.id);
    setError(null);
    setSubmissions((rows) =>
      rows.map((row) => (row.id === submission.id ? { ...row, handled } : row)),
    );
    try {
      await api.setSubmissionHandled(
        siteId,
        submission.formId,
        submission.id,
        handled,
      );
    } catch (err) {
      setSubmissions((rows) =>
        rows.map((row) =>
          row.id === submission.id
            ? { ...row, handled: submission.handled }
            : row,
        ),
      );
      setError(sitesMessage(err, strings.sitesSubmissionSaveFailed));
    } finally {
      setBusyId(null);
    }
  }

  async function exportCsv() {
    if (site === null) return;
    setExporting(true);
    setError(null);
    try {
      const csv = await api.submissionsCsv(siteId);
      saveTextFile(
        csv,
        `submissions-${site.subdomain}.csv`,
        "text/csv;charset=utf-8",
      );
    } catch (err) {
      setError(sitesMessage(err, strings.sitesSubmissionsExportFailed));
    } finally {
      setExporting(false);
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-[90rem] flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8">
      <header className="flex flex-col gap-4 rounded-2xl border border-subtle bg-surface px-5 py-4 shadow-sm sm:flex-row sm:items-center sm:justify-between">
        <div className="flex min-w-0 items-center gap-4">
          <Link
            to=".."
            relative="path"
            className="inline-flex min-h-10 shrink-0 items-center gap-2 rounded-xl px-3 text-sm font-semibold text-secondary no-underline transition-colors hover:bg-app hover:text-primary"
          >
            <ArrowLeft size={16} aria-hidden="true" />
            {strings.sitesBackToSite}
          </Link>
          <span className="h-8 w-px bg-subtle" aria-hidden="true" />
          <div className="min-w-0">
            <h1 className="text-xl font-bold tracking-tight text-primary">
              {strings.sitesSubmissions}
            </h1>
            {site !== null && (
              <p className="truncate text-sm text-secondary">{site.name}</p>
            )}
          </div>
        </div>
        <div className="flex items-center gap-3 self-end sm:self-auto">
          {loading && <Spinner size={16} />}
          <Button
            variant="secondary"
            size="sm"
            icon={<Download size={14} />}
            disabled={site === null || submissions.length === 0 || exporting}
            onClick={() => void exportCsv()}
          >
            {exporting
              ? strings.sitesExportingSubmissions
              : strings.sitesExportSubmissions}
          </Button>
        </div>
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {loading ? (
        <div className="flex min-h-[24rem] items-center justify-center rounded-2xl border border-subtle bg-surface">
          <Spinner size={24} />
        </div>
      ) : submissions.length === 0 ? (
        <EmptyState
          Icon={Inbox}
          title={strings.sitesNoSubmissionsTitle}
          body={strings.sitesNoSubmissionsBody}
          cta={strings.sitesOpenPages}
          onCta={() => navigate(`/sites/${encodeURIComponent(siteId)}`)}
        />
      ) : (
        <div className="grid min-h-[34rem] overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm lg:grid-cols-[21rem_minmax(0,1fr)]">
          <section
            className="border-b border-subtle bg-app/35 p-2 lg:border-b-0 lg:border-r"
            aria-label={strings.sitesSubmissionList}
          >
            {submissions.map((submission) => (
              <button
                type="button"
                key={submission.id}
                className={`mb-1 flex w-full flex-col gap-1 rounded-xl border px-4 py-3 text-left transition-colors ${
                  selectedId === submission.id
                    ? "border-accent/25 bg-accent-soft text-primary shadow-sm"
                    : "border-transparent bg-transparent text-primary hover:border-subtle hover:bg-surface"
                }`}
                onClick={() => setSelectedId(submission.id)}
                aria-pressed={selectedId === submission.id}
              >
                <span className="flex min-w-0 items-center justify-between gap-3">
                  <strong className="truncate text-sm font-semibold">
                    {submission.senderName}
                  </strong>
                  <time
                    className="shrink-0 text-xs text-muted"
                    dateTime={submission.receivedAt}
                  >
                    {received.format(new Date(submission.receivedAt))}
                  </time>
                </span>
                <span className="truncate text-sm text-secondary">
                  {submission.senderEmail}
                </span>
                <span className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted">
                  <span className="truncate">{submission.formName}</span>
                  {leadBySubmission.has(submission.id) && (
                    <span className="rounded-full bg-surface px-2 py-1 font-medium text-secondary ring-1 ring-inset ring-subtle">
                      {strings.sitesInSales}
                    </span>
                  )}
                  <span
                    className={`ml-auto rounded-full px-2 py-1 font-semibold ${
                      submission.handled
                        ? "bg-neutral-soft text-secondary"
                        : "bg-accent-soft text-accent"
                    }`}
                  >
                    {submission.handled
                      ? strings.sitesHandled
                      : strings.sitesNeedsReply}
                  </span>
                </span>
              </button>
            ))}
          </section>

          {selected !== null && (
            <article
              className="flex min-w-0 flex-col p-5 sm:p-7 lg:p-8"
              aria-label={strings.sitesSubmissionDetail}
            >
              <header className="flex flex-col gap-4 border-b border-subtle pb-5 sm:flex-row sm:items-start sm:justify-between">
                <div className="min-w-0">
                  <h2 className="truncate text-2xl font-bold tracking-tight text-primary">
                    {selected.senderName}
                  </h2>
                  <a
                    className="mt-1 inline-block truncate text-sm text-secondary no-underline hover:text-accent"
                    href={`mailto:${selected.senderEmail}`}
                  >
                    {selected.senderEmail}
                  </a>
                </div>
                <Button
                  variant={selected.handled ? "ghost" : "primary"}
                  size="sm"
                  icon={
                    selected.handled ? (
                      <RotateCcw size={14} />
                    ) : (
                      <Check size={14} />
                    )
                  }
                  disabled={busyId === selected.id}
                  onClick={() => void setHandled(selected, !selected.handled)}
                >
                  {selected.handled
                    ? strings.sitesReopenSubmission
                    : strings.sitesMarkHandled}
                </Button>
              </header>
              <dl className="grid gap-3 py-5 sm:grid-cols-2">
                <div className="rounded-xl bg-app px-4 py-3">
                  <dt className="text-xs font-semibold uppercase tracking-wide text-muted">
                    {strings.sitesForm}
                  </dt>
                  <dd className="mt-1 text-sm font-medium text-primary">
                    {selected.formName}
                  </dd>
                </div>
                <div className="rounded-xl bg-app px-4 py-3">
                  <dt className="text-xs font-semibold uppercase tracking-wide text-muted">
                    {strings.sitesReceived}
                  </dt>
                  <dd className="mt-1 text-sm font-medium text-primary">
                    {received.format(new Date(selected.receivedAt))}
                  </dd>
                </div>
              </dl>
              <p className="min-h-32 whitespace-pre-wrap break-words rounded-2xl border border-subtle bg-surface px-5 py-4 text-base leading-7 text-primary">
                {selected.message}
              </p>

              {salesVisible && (
                <section
                  className="mt-auto flex flex-col gap-4 border-t border-subtle pt-6 sm:flex-row sm:items-center sm:justify-between"
                  aria-label={strings.sitesHandoffSection}
                >
                  {selectedLead === undefined ? (
                    <>
                      <div className="min-w-0">
                        <strong className="text-sm font-semibold text-primary">
                          {strings.sitesHandoffSection}
                        </strong>
                        <p className="mt-1 text-sm text-secondary">
                          {strings.sitesHandoffInvite}
                        </p>
                      </div>
                      <Button
                        variant="secondary"
                        size="sm"
                        icon={<Handshake size={14} />}
                        onClick={() => setHandingOff(selected)}
                      >
                        {strings.sitesHandoffSubmit}
                      </Button>
                    </>
                  ) : (
                    <>
                      <div className="min-w-0">
                        {/* The opportunity as CRM holds it right now — this
                            screen keeps no copy of a title or a value. */}
                        <strong className="block truncate text-sm font-semibold text-primary">
                          {selectedLead.deal.title}
                        </strong>
                        <p className="mt-1 text-sm text-secondary">
                          {strings.sitesLeadStanding(
                            dealState(selectedLead.deal.state),
                            funnelMoney(
                              selectedLead.deal.valueCents,
                              selectedLead.deal.currency,
                            ),
                          )}
                        </p>
                      </div>
                      <Button
                        variant="ghost"
                        size="sm"
                        icon={<Unlink size={14} />}
                        disabled={busyId === selected.id}
                        onClick={() => void unlink(selectedLead)}
                      >
                        {strings.sitesUnlinkLead}
                      </Button>
                    </>
                  )}
                </section>
              )}
            </article>
          )}
        </div>
      )}

      {handingOff !== null && (
        <HandoffDialog
          siteId={siteId}
          submission={handingOff}
          onClose={() => setHandingOff(null)}
          onLinked={(link) => {
            setHandingOff(null);
            setLeads((current) => [link, ...current]);
            setSelectedId(link.submissionId);
          }}
        />
      )}
    </div>
  );
}

/** What the server says the opportunity's state is — never re-derived from a
 *  column's flags, which is how a client and a server end up disagreeing about
 *  whether something was won. */
function dealState(state: SiteLeadLink["deal"]["state"]): string {
  if (state === "won") return strings.sitesLeadWon;
  if (state === "lost") return strings.sitesLeadLost;
  return strings.sitesLeadOpen;
}
