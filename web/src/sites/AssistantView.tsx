// The site assistant's one admin screen (ADR 0040, item S3.02d): the switch,
// the monthly budget, and the reading list live together — because "what may
// it say", "what may it cost" and "what may it read" are one decision about
// one stranger-facing surface, and splitting them is how a bot gets switched
// on with a blank budget or an unread source list.
//
// The boundary rule is a sentence, not a permission model, and this screen
// says it in those words every time a source is published: **anyone on the
// internet will be able to read this.** It is never softened, never shown
// once and remembered — it stands above the publish button on every pass.
import { useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  ArrowLeft,
  BookOpenCheck,
  Bot,
  CalendarCheck,
  CircleSlash,
  Clock,
  FileText,
  Globe2,
  ListChecks,
  MessageCircle,
  Newspaper,
  Ticket,
  UserCheck,
  UserPlus,
  Users,
  X,
} from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { AssistantAppearance } from "./AssistantAppearance";
import { funnelMoney } from "./funnelReading";
import { KnowledgePickerDialog } from "./KnowledgePickerDialog";
import { ErrorBanner } from "./parts";
import type {
  SiteChatAction,
  SiteChatSettings,
  SiteDetail,
  SiteKnowledgeSource,
} from "./types";

const styles = {
  page: "mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 py-6 sm:px-6 lg:px-8",
  header: "flex flex-wrap items-center gap-4 border-b border-subtle pb-5",
  backLink:
    "inline-flex min-h-10 items-center gap-2 rounded-xl px-3 text-sm font-semibold text-secondary no-underline transition hover:bg-muted hover:text-primary",
  siteHead: "min-w-0 flex-1",
  title: "text-2xl font-semibold tracking-tight text-primary",
  mono: "mt-1 block truncate text-sm text-secondary",
  pageBody: "grid gap-5 lg:grid-cols-2",
  languagePanel:
    "flex min-w-0 flex-col gap-5 rounded-2xl border border-subtle bg-surface p-5 shadow-sm sm:p-6",
  languagePanelIntro: "flex items-start gap-3 border-b border-subtle pb-4",
  languagePanelIcon:
    "grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent [&_svg]:size-5",
  languageTitle: "text-base font-semibold text-primary",
  languageHint: "mt-1 text-sm leading-6 text-secondary",
  assistantSwitch:
    "flex min-h-12 cursor-pointer items-center justify-between gap-4 rounded-xl bg-muted px-4 text-sm font-semibold text-primary [&_input]:size-5 [&_input]:accent-[var(--accent)]",
  languageControls: "grid items-end gap-3 sm:grid-cols-[minmax(0,1fr)_auto]",
  languageControl:
    "flex min-w-0 flex-col gap-2 text-xs font-semibold uppercase tracking-wide text-secondary",
  input:
    "min-h-11 w-full rounded-xl border border-default bg-surface px-3.5 py-2.5 text-base font-normal normal-case tracking-normal text-primary outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/15 disabled:bg-muted",
  hint: "text-sm leading-6 text-secondary",
  assistantWarning:
    "rounded-xl border border-warning/20 bg-warning/10 px-4 py-3 text-sm leading-6 text-primary",
  publishError: "text-sm font-medium text-danger",
  assistantSources:
    "flex list-none flex-col divide-y divide-subtle overflow-hidden rounded-xl border border-subtle p-0",
  assistantSource:
    "flex min-w-0 flex-wrap items-center gap-3 bg-surface px-4 py-3 text-sm text-primary [&>svg]:shrink-0 [&>svg]:text-secondary",
  badge:
    "inline-flex min-h-7 items-center rounded-full bg-muted px-2.5 text-xs font-semibold text-secondary",
  assistantSourceMeta: "ml-auto text-xs text-tertiary",
};

const added = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });
const acted = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

/** The transcript entry as one sentence: the act, the fact it used, and the
 *  pages that fact came from. `null` for a kind a later release added — the
 *  entry is skipped rather than broken on. */
function actionSentence(action: SiteChatAction): string | null {
  switch (action.kind) {
    case "answered": {
      const pages = action.citations
        .map((c) => (c.path === null ? c.title : `${c.title} (${c.path})`))
        .join(", ");
      return pages === ""
        ? strings.sitesAssistantDidAnswered
        : strings.sitesAssistantDidAnsweredUsing(pages);
    }
    case "refused":
      return strings.sitesAssistantDidRefused;
    case "booking_offered":
      return strings.sitesAssistantDidBookingOffered(action.fact ?? "");
    case "booked":
      return strings.sitesAssistantDidBooked(
        action.fact ?? "",
        action.slotAt === null ? "" : acted.format(new Date(action.slotAt)),
      );
    case "lead_offered":
      return strings.sitesAssistantDidLeadOffered;
    case "lead_saved":
      return strings.sitesAssistantDidLeadSaved;
    case "lead_known":
      return strings.sitesAssistantDidLeadKnown;
    case "tickets_offered":
      return strings.sitesAssistantDidTicketsOffered(action.fact ?? "");
    default:
      return null;
  }
}

function actionIcon(kind: SiteChatAction["kind"]) {
  switch (kind) {
    case "answered":
      return MessageCircle;
    case "refused":
      return CircleSlash;
    case "booking_offered":
      return Clock;
    case "booked":
      return CalendarCheck;
    case "lead_offered":
      return UserPlus;
    case "lead_saved":
      return UserCheck;
    case "tickets_offered":
      return Ticket;
    default:
      return Users;
  }
}

export function AssistantView() {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [settings, setSettings] = useState<SiteChatSettings | null>(null);
  const [sources, setSources] = useState<SiteKnowledgeSource[]>([]);
  const [actions, setActions] = useState<SiteChatAction[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // The form's two fields. The budget is edited in euros and stored in
  // integer cents; parsing happens once, on save, so a half-typed number
  // never fights the field.
  const [enabled, setEnabled] = useState(false);
  const [budgetInput, setBudgetInput] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const [picking, setPicking] = useState(false);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [sourceError, setSourceError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [detail, chat, knowledge, did] = await Promise.all([
        api.site(siteId),
        api.chatSettings(siteId),
        api.chatKnowledge(siteId),
        api.chatActions(siteId),
      ]);
      setSite(detail);
      setSettings(chat);
      setSources(knowledge);
      setActions(did);
      setEnabled(chat.enabled);
      setBudgetInput((chat.monthlyCeilingCents / 100).toString());
      setError(null);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesAssistantLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function save() {
    const euros = Number(budgetInput.replace(",", "."));
    if (!Number.isFinite(euros)) {
      setSaveError(strings.sitesAssistantBudgetNotANumber);
      return;
    }
    setSaving(true);
    setSaved(false);
    setSaveError(null);
    try {
      const next = await api.setChatSettings(
        siteId,
        enabled,
        Math.round(euros * 100),
      );
      setSettings(next);
      setEnabled(next.enabled);
      setBudgetInput((next.monthlyCeilingCents / 100).toString());
      setSaved(true);
    } catch (err) {
      setSaveError(sitesMessage(err, strings.sitesAssistantSaveFailed));
    } finally {
      setSaving(false);
    }
  }

  async function withdraw(source: SiteKnowledgeSource) {
    setRemovingId(source.id);
    setSourceError(null);
    try {
      await api.removeChatKnowledge(siteId, source.id);
      setSources((current) => current.filter((row) => row.id !== source.id));
    } catch (err) {
      setSourceError(sitesMessage(err, strings.sitesAssistantWithdrawFailed));
    } finally {
      setRemovingId(null);
    }
  }

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size={16} aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesAssistantTitle}</h1>
          {site !== null && <span className={styles.mono}>{site.name}</span>}
        </div>
        {loading && <Spinner size={16} />}
      </header>

      <div className={styles.pageBody}>
        {error !== null && <ErrorBanner message={error} />}

        {loading && (
          <div
            className="col-span-full flex min-h-72 items-center justify-center rounded-2xl border border-subtle bg-surface shadow-sm"
            role="status"
            aria-label={strings.sitesAssistantTitle}
          >
            <Spinner size={22} />
          </div>
        )}

        {settings !== null && (
          <section
            className={styles.languagePanel}
            aria-labelledby="assistant-settings-title"
          >
            <div className={styles.languagePanelIntro}>
              <span className={styles.languagePanelIcon} aria-hidden="true">
                <Bot />
              </span>
              <div>
                <h2
                  id="assistant-settings-title"
                  className={styles.languageTitle}
                >
                  {strings.sitesAssistantSwitchTitle}
                </h2>
                <p className={styles.languageHint}>
                  {strings.sitesAssistantSwitchHint}
                </p>
              </div>
            </div>

            <label className={styles.assistantSwitch}>
              <input
                type="checkbox"
                checked={enabled}
                disabled={saving}
                onChange={(event) => {
                  setEnabled(event.target.checked);
                  setSaved(false);
                }}
              />
              <span>{strings.sitesAssistantEnable}</span>
            </label>

            <div className={styles.languageControls}>
              <label className={styles.languageControl}>
                <span>{strings.sitesAssistantBudgetLabel}</span>
                <input
                  className={styles.input}
                  type="number"
                  min="1"
                  step="0.01"
                  inputMode="decimal"
                  value={budgetInput}
                  disabled={saving}
                  onChange={(event) => {
                    setBudgetInput(event.target.value);
                    setSaved(false);
                  }}
                />
              </label>
              <span className={styles.hint}>
                {strings.sitesAssistantBudgetHint(
                  funnelMoney(settings.defaultCeilingCents, "EUR"),
                )}
              </span>
              <Button size="sm" disabled={saving} onClick={() => void save()}>
                {strings.sitesAssistantSave}
              </Button>
            </div>

            <p className={styles.hint}>
              {strings.sitesAssistantSpent(
                funnelMoney(settings.spentCents, "EUR"),
                funnelMoney(settings.monthlyCeilingCents, "EUR"),
              )}
            </p>
            {settings.ceilingHit && (
              <p className={styles.assistantWarning} role="alert">
                {strings.sitesAssistantCeilingHit}
              </p>
            )}
            {saveError !== null && (
              <span className={styles.publishError} role="alert">
                {saveError}
              </span>
            )}
            {saved && <span role="status">{strings.sitesAssistantSaved}</span>}
          </section>
        )}

        {settings !== null && (
          <section
            className={styles.languagePanel}
            aria-labelledby="assistant-reads-title"
          >
            <div className={styles.languagePanelIntro}>
              <span className={styles.languagePanelIcon} aria-hidden="true">
                <BookOpenCheck />
              </span>
              <div>
                <h2 id="assistant-reads-title" className={styles.languageTitle}>
                  {strings.sitesAssistantReadsTitle}
                </h2>
                {/* The whole permission model, in the one sentence a customer
                    can be shown after an incident (ADR 0040 §1). */}
                <p className={styles.languageHint}>
                  {strings.sitesAssistantReadsRule}
                </p>
              </div>
            </div>

            <ul className={styles.assistantSources}>
              <li className={styles.assistantSource}>
                <Globe2 size={16} aria-hidden="true" />
                <span>{strings.sitesAssistantReadsPublishedSite}</span>
                <span className={styles.badge}>
                  {strings.sitesAssistantAlwaysRead}
                </span>
              </li>
              <li className={styles.assistantSource}>
                <Newspaper size={16} aria-hidden="true" />
                <span>{strings.sitesAssistantReadsPublishedPosts}</span>
                <span className={styles.badge}>
                  {strings.sitesAssistantAlwaysRead}
                </span>
              </li>
              {sources.map((source) => (
                <li className={styles.assistantSource} key={source.id}>
                  <FileText size={16} aria-hidden="true" />
                  <span>{source.title}</span>
                  {source.trashed ? (
                    <span className={styles.badge}>
                      {strings.sitesAssistantTrashed}
                    </span>
                  ) : (
                    <span className={styles.assistantSourceMeta}>
                      {strings.sitesAssistantAddedOn(
                        added.format(new Date(source.addedAt)),
                      )}
                    </span>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<X size="var(--icon-size-inline)" />}
                    disabled={removingId === source.id}
                    onClick={() => void withdraw(source)}
                  >
                    {strings.sitesAssistantWithdraw(source.title)}
                  </Button>
                </li>
              ))}
            </ul>
            {sources.length === 0 && (
              <p className={styles.hint}>{strings.sitesAssistantNoKnowledge}</p>
            )}
            {sourceError !== null && (
              <span className={styles.publishError} role="alert">
                {sourceError}
              </span>
            )}

            {/* The sentence stands above the button on every pass — it is the
                design, not a caveat (ADR 0040 §1). */}
            <p className={styles.assistantWarning}>
              {strings.sitesAssistantInternetWarning}
            </p>
            <div>
              <Button size="sm" onClick={() => setPicking(true)}>
                {strings.sitesAssistantPublishDocument}
              </Button>
            </div>
          </section>
        )}

        {settings !== null && (
          <section
            className={styles.languagePanel}
            aria-labelledby="assistant-did-title"
          >
            <div className={styles.languagePanelIntro}>
              <span className={styles.languagePanelIcon} aria-hidden="true">
                <ListChecks />
              </span>
              <div>
                <h2 id="assistant-did-title" className={styles.languageTitle}>
                  {strings.sitesAssistantDidTitle}
                </h2>
                {/* The accountability half of ADR 0040: every act, the fact
                    it used, the page that fact came from — and never the
                    conversation's words or the visitor (S3.03e). */}
                <p className={styles.languageHint}>
                  {strings.sitesAssistantDidHint}
                </p>
              </div>
            </div>

            {actions.length === 0 ? (
              <p className={styles.hint}>{strings.sitesAssistantDidEmpty}</p>
            ) : (
              <ul className={styles.assistantSources}>
                {actions.map((action) => {
                  const sentence = actionSentence(action);
                  if (sentence === null) {
                    return null;
                  }
                  const Icon = actionIcon(action.kind);
                  return (
                    <li className={styles.assistantSource} key={action.id}>
                      <Icon size={16} aria-hidden="true" />
                      <span>{sentence}</span>
                      <span className={styles.assistantSourceMeta}>
                        {acted.format(new Date(action.occurredAt))}
                      </span>
                    </li>
                  );
                })}
              </ul>
            )}
          </section>
        )}

        {settings !== null && (
          <AssistantAppearance siteId={siteId} site={site} />
        )}
      </div>

      {picking && (
        <KnowledgePickerDialog
          siteId={siteId}
          onClose={() => setPicking(false)}
          onPublished={(source) => {
            setPicking(false);
            setSources((current) => [...current, source]);
          }}
        />
      )}
    </div>
  );
}
