// A website's contact inbox: the queue and the selected message share one
// surface, like Mail, so reading and resolving a visitor request never
// navigates away or hides the next action in a menu.
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, Check, Inbox, RotateCcw } from "lucide-react";
import { Link, useNavigate, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { EmptyState, ErrorBanner } from "./parts";
import type { SiteDetail, SiteSubmission } from "./types";
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
  const [error, setError] = useState<string | null>(null);

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

  const selected = useMemo(
    () =>
      submissions.find((submission) => submission.id === selectedId) ?? null,
    [selectedId, submissions],
  );

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
        {loading && <Spinner size={16} />}
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
            </article>
          )}
        </div>
      )}
    </div>
  );
}
