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
import styles from "./SitesModule.module.css";

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
  const selectedLead = selected === null ? undefined : leadBySubmission.get(selected.id);

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
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size={16} aria-hidden="true" />
          {strings.sitesBackToSite}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesSubmissions}</h1>
          {site !== null && (
            <span className={styles.submissionSiteName}>{site.name}</span>
          )}
        </div>
        <div className={styles.headerActions}>
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

      {!loading && submissions.length === 0 ? (
        <EmptyState
          Icon={Inbox}
          title={strings.sitesNoSubmissionsTitle}
          body={strings.sitesNoSubmissionsBody}
          cta={strings.sitesOpenPages}
          onCta={() => navigate(`/sites/${encodeURIComponent(siteId)}`)}
        />
      ) : (
        <div className={styles.submissionsLayout}>
          <section
            className={styles.submissionList}
            aria-label={strings.sitesSubmissionList}
          >
            {submissions.map((submission) => (
              <button
                type="button"
                key={submission.id}
                className={`${styles.submissionRow} ${
                  selectedId === submission.id
                    ? styles.submissionRowSelected
                    : ""
                }`}
                onClick={() => setSelectedId(submission.id)}
                aria-pressed={selectedId === submission.id}
              >
                <span className={styles.submissionRowTop}>
                  <strong>{submission.senderName}</strong>
                  <time dateTime={submission.receivedAt}>
                    {received.format(new Date(submission.receivedAt))}
                  </time>
                </span>
                <span className={styles.submissionEmail}>
                  {submission.senderEmail}
                </span>
                <span className={styles.submissionRowBottom}>
                  <span>{submission.formName}</span>
                  {leadBySubmission.has(submission.id) && (
                    <span className={styles.submissionLeadChip}>
                      {strings.sitesInSales}
                    </span>
                  )}
                  <span
                    className={
                      submission.handled ? styles.handled : styles.open
                    }
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
              className={styles.submissionDetail}
              aria-label={strings.sitesSubmissionDetail}
            >
              <header className={styles.submissionDetailHead}>
                <div>
                  <h2>{selected.senderName}</h2>
                  <a href={`mailto:${selected.senderEmail}`}>
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
              <dl className={styles.submissionMeta}>
                <div>
                  <dt>{strings.sitesForm}</dt>
                  <dd>{selected.formName}</dd>
                </div>
                <div>
                  <dt>{strings.sitesReceived}</dt>
                  <dd>{received.format(new Date(selected.receivedAt))}</dd>
                </div>
              </dl>
              <p className={styles.submissionMessage}>{selected.message}</p>

              {salesVisible && (
                <section
                  className={styles.submissionSales}
                  aria-label={strings.sitesHandoffSection}
                >
                  {selectedLead === undefined ? (
                    <>
                      <div className={styles.submissionSalesText}>
                        <strong>{strings.sitesHandoffSection}</strong>
                        <p>{strings.sitesHandoffInvite}</p>
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
                      <div className={styles.submissionSalesText}>
                        {/* The opportunity as CRM holds it right now — this
                            screen keeps no copy of a title or a value. */}
                        <strong>{selectedLead.deal.title}</strong>
                        <p>
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
