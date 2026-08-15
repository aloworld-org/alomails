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
import { ArrowLeft, BookOpenCheck, Bot, FileText, Globe2, Newspaper, X } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { AssistantAppearance } from "./AssistantAppearance";
import { funnelMoney } from "./funnelReading";
import { KnowledgePickerDialog } from "./KnowledgePickerDialog";
import { ErrorBanner } from "./parts";
import type { SiteChatSettings, SiteDetail, SiteKnowledgeSource } from "./types";
import styles from "./SitesModule.module.css";

const added = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });

export function AssistantView() {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [settings, setSettings] = useState<SiteChatSettings | null>(null);
  const [sources, setSources] = useState<SiteKnowledgeSource[]>([]);
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
      const [detail, chat, knowledge] = await Promise.all([
        api.site(siteId),
        api.chatSettings(siteId),
        api.chatKnowledge(siteId),
      ]);
      setSite(detail);
      setSettings(chat);
      setSources(knowledge);
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
      const next = await api.setChatSettings(siteId, enabled, Math.round(euros * 100));
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
                <h2 id="assistant-settings-title" className={styles.languageTitle}>
                  {strings.sitesAssistantSwitchTitle}
                </h2>
                <p className={styles.languageHint}>{strings.sitesAssistantSwitchHint}</p>
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
                <p className={styles.languageHint}>{strings.sitesAssistantReadsRule}</p>
              </div>
            </div>

            <ul className={styles.assistantSources}>
              <li className={styles.assistantSource}>
                <Globe2 size={16} aria-hidden="true" />
                <span>{strings.sitesAssistantReadsPublishedSite}</span>
                <span className={styles.badge}>{strings.sitesAssistantAlwaysRead}</span>
              </li>
              <li className={styles.assistantSource}>
                <Newspaper size={16} aria-hidden="true" />
                <span>{strings.sitesAssistantReadsPublishedPosts}</span>
                <span className={styles.badge}>{strings.sitesAssistantAlwaysRead}</span>
              </li>
              {sources.map((source) => (
                <li className={styles.assistantSource} key={source.id}>
                  <FileText size={16} aria-hidden="true" />
                  <span>{source.title}</span>
                  {source.trashed ? (
                    <span className={styles.badge}>{strings.sitesAssistantTrashed}</span>
                  ) : (
                    <span className={styles.assistantSourceMeta}>
                      {strings.sitesAssistantAddedOn(added.format(new Date(source.addedAt)))}
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

        {settings !== null && <AssistantAppearance siteId={siteId} site={site} />}
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
