// The assistant's appearance panel (ADR 0040 §5, item S3.02g): the words and
// bounded choices of the public widget, edited beside a live preview of the
// REAL widget — the same markup and stylesheet the public service injects,
// rendered server-side from the unsaved draft — so what the owner sees is
// what a visitor gets, not a lookalike.
//
// Three of the queue item's demands shape this file: the welcome field is
// PRE-FILLED with the written default rather than shown as an empty box; the
// suggested questions can be drafted from the site's own pages (FAQ entries
// verbatim, then one canonical question per present section kind — local and
// deterministic, no model call); and the accessibility facts are shown in
// the screen — the measured contrast of the chosen colour among them —
// rather than discovered after publishing.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Brush, Sparkles, Trash2, Upload } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { accentContrast } from "./accentContrast";
import { sitesMessage, useSitesApi } from "./api";
import { ErrorBanner } from "./parts";
import { draftSuggestedQuestions } from "./suggestQuestions";
import type {
  SiteChatAccent,
  SiteChatAppearance,
  SiteChatAppearanceView,
  SiteChatCorner,
  SiteChatIcon,
  SiteChatTone,
  SiteDetail,
  ThemePreset,
} from "./types";
import styles from "./SitesModule.module.css";

/** How long a pause in typing is before the preview re-renders. */
const PREVIEW_DEBOUNCE_MS = 400;

/** A trimmed field as the wire wants it: `null` for "use the default". */
function trimmedOrNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/** The editable form state — every text field a plain string, so a
 *  half-typed value never fights the input. */
interface Fields {
  botName: string;
  avatarBlobId: string | null;
  welcome: string;
  questions: string[];
  tone: SiteChatTone;
  toneNote: string;
  corner: SiteChatCorner;
  icon: SiteChatIcon;
  autoOpen: boolean;
  offline: string;
  accent: SiteChatAccent;
}

/** The form state a stored view opens as. The welcome box is pre-filled with
 *  the written default when nothing is stored — never an empty box. */
function fieldsOf(view: SiteChatAppearanceView): Fields {
  const questions = [...view.suggestedQuestions];
  while (questions.length < view.limits.suggestedQuestions) questions.push("");
  return {
    botName: view.botName ?? "",
    avatarBlobId: view.avatarBlobId,
    welcome: view.welcome ?? view.defaults.welcome,
    questions,
    tone: view.tone,
    toneNote: view.toneNote ?? "",
    corner: view.launcherCorner,
    icon: view.launcherIcon,
    autoOpen: view.autoOpen,
    offline: view.offlineMessage ?? "",
    accent: view.accent,
  };
}

/** The wire appearance a form state saves and previews as. A welcome left at
 *  the written default is sent as absent, so the widget keeps speaking the
 *  site's language rather than freezing one translation of it. */
function appearanceOf(fields: Fields, defaultWelcome: string): SiteChatAppearance {
  const welcome = trimmedOrNull(fields.welcome);
  return {
    botName: trimmedOrNull(fields.botName),
    avatarBlobId: fields.avatarBlobId,
    welcome: welcome === defaultWelcome.trim() ? null : welcome,
    suggestedQuestions: fields.questions
      .map((question) => question.trim())
      .filter((question) => question !== ""),
    tone: fields.tone,
    toneNote: trimmedOrNull(fields.toneNote),
    launcherCorner: fields.corner,
    launcherIcon: fields.icon,
    autoOpen: fields.autoOpen,
    offlineMessage: trimmedOrNull(fields.offline),
    accent: fields.accent,
  };
}

/** One bounded choice as a radio row. */
function ChoiceGroup<Value extends string>({
  legend,
  hint,
  name,
  value,
  options,
  disabled,
  onChange,
}: {
  legend: string;
  hint?: string;
  name: string;
  value: Value;
  options: Array<{ value: Value; label: string }>;
  disabled: boolean;
  onChange: (value: Value) => void;
}) {
  return (
    <fieldset className={styles.appearanceChoices}>
      <legend className={styles.label}>{legend}</legend>
      {hint !== undefined && <span className={styles.hint}>{hint}</span>}
      <div className={styles.appearanceChoiceRow}>
        {options.map((option) => (
          <label key={option.value} className={styles.appearanceChoice}>
            <input
              type="radio"
              name={name}
              checked={value === option.value}
              disabled={disabled}
              onChange={() => onChange(option.value)}
            />
            <span>{option.label}</span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}

/** The appearance and voice panel of the assistant screen. */
export function AssistantAppearance({
  siteId,
  site,
}: {
  siteId: string;
  site: SiteDetail | null;
}) {
  const api = useSitesApi();
  const jmap = useJmapClient();
  const [view, setView] = useState<SiteChatAppearanceView | null>(null);
  const [fields, setFields] = useState<Fields | null>(null);
  const [presets, setPresets] = useState<ThemePreset[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const [previewHtml, setPreviewHtml] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const previewCall = useRef(0);

  const [suggesting, setSuggesting] = useState(false);
  const [suggestNote, setSuggestNote] = useState<string | null>(null);

  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);

  useEffect(() => {
    let stale = false;
    Promise.all([api.chatAppearance(siteId), api.themePresets()]).then(
      ([appearance, shipped]) => {
        if (stale) return;
        setView(appearance);
        setFields(fieldsOf(appearance));
        setPresets(shipped);
        setLoadError(null);
      },
      (error: unknown) => {
        if (!stale) setLoadError(sitesMessage(error, strings.sitesAssistantLoadFailed));
      },
    );
    return () => {
      stale = true;
    };
  }, [api, siteId]);

  const edit = useCallback((change: Partial<Fields>) => {
    setSaved(false);
    setFields((current) => (current === null ? null : { ...current, ...change }));
  }, []);

  // The draft the preview and the save both send — one builder, so the
  // preview can never show a value the save would spell differently.
  const draftJson = useMemo(
    () =>
      view === null || fields === null
        ? null
        : JSON.stringify(appearanceOf(fields, view.defaults.welcome)),
    [view, fields],
  );

  useEffect(() => {
    if (draftJson === null) return undefined;
    const call = ++previewCall.current;
    const handle = setTimeout(() => {
      api.chatAppearancePreview(siteId, JSON.parse(draftJson) as SiteChatAppearance).then(
        (html) => {
          if (previewCall.current !== call) return;
          setPreviewHtml(html);
          setPreviewError(null);
        },
        (error: unknown) => {
          if (previewCall.current !== call) return;
          setPreviewError(sitesMessage(error, strings.sitesAssistantPreviewFailed));
        },
      );
    }, PREVIEW_DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [api, siteId, draftJson]);

  async function save() {
    if (view === null || fields === null) return;
    setSaving(true);
    setSaved(false);
    setSaveError(null);
    try {
      const stored = await api.setChatAppearance(
        siteId,
        appearanceOf(fields, view.defaults.welcome),
      );
      setView(stored);
      setFields(fieldsOf(stored));
      setSaved(true);
    } catch (error) {
      setSaveError(sitesMessage(error, strings.sitesAssistantSaveFailed));
    } finally {
      setSaving(false);
    }
  }

  /** Drafts suggested questions from the site's own pages into the EMPTY
   *  slots — a typed question is the owner's and is never overwritten. */
  async function suggest() {
    if (view === null || fields === null) return;
    setSuggesting(true);
    setSuggestNote(null);
    try {
      const pages = await api.pages(siteId);
      const details = await Promise.all(
        pages.slice(0, 12).map((page) => api.page(siteId, page.id)),
      );
      const kept = fields.questions.map((question) => question.trim());
      const drafts = draftSuggestedQuestions(
        details,
        view.limits.suggestedQuestions,
        view.limits.suggestedQuestionChars,
      ).filter(
        (draft) =>
          !kept.some((question) => question.toLowerCase() === draft.toLowerCase()),
      );
      if (drafts.length === 0) {
        setSuggestNote(strings.sitesAssistantSuggestedNone);
        return;
      }
      const questions = fields.questions.map((question) =>
        question.trim() === "" ? (drafts.shift() ?? question) : question,
      );
      edit({ questions });
      setSuggestNote(strings.sitesAssistantSuggestedApplied);
    } catch (error) {
      setSuggestNote(sitesMessage(error, strings.sitesAssistantSuggestFailed));
    } finally {
      setSuggesting(false);
    }
  }

  function uploadAvatar(file: File) {
    setUploading(true);
    setUploadError(null);
    jmap.driveUploadBlob(null, null, file).then(
      ({ blobId }) => {
        edit({ avatarBlobId: blobId });
        setUploading(false);
      },
      () => {
        setUploadError(strings.sitesUploadFailed);
        setUploading(false);
      },
    );
  }

  // The measured contrast of the chosen accent on the site's own preset —
  // the number the accessibility box shows. The server proves the guarantee;
  // this shows it.
  const palette = useMemo(() => {
    if (presets.length === 0) return null;
    const chosen = site?.theme.preset;
    return (presets.find((preset) => preset.id === chosen) ?? presets[0])?.palette ?? null;
  }, [presets, site]);
  const contrast =
    palette === null || fields === null ? null : accentContrast(fields.accent, palette);

  const fileInput = useRef<HTMLInputElement>(null);
  const busy = saving || view === null;

  return (
    <section className={styles.languagePanel} aria-labelledby="assistant-look-title">
      <div className={styles.languagePanelIntro}>
        <span className={styles.languagePanelIcon} aria-hidden="true">
          <Brush />
        </span>
        <div>
          <h2 id="assistant-look-title" className={styles.languageTitle}>
            {strings.sitesAssistantLookTitle}
          </h2>
          <p className={styles.languageHint}>{strings.sitesAssistantLookHint}</p>
        </div>
      </div>

      {loadError !== null && <ErrorBanner message={loadError} />}
      {view === null && loadError === null && <Spinner size={18} />}

      {view !== null && fields !== null && (
        <div className={styles.appearanceLayout}>
          <div className={styles.appearanceFields}>
            <div className={styles.appearanceField}>
              <label className={styles.appearanceField}>
                <span className={styles.label}>{strings.sitesAssistantBotNameLabel}</span>
                <input
                  className={styles.input}
                  type="text"
                  value={fields.botName}
                  maxLength={view.limits.botNameChars}
                  placeholder={view.defaults.botName}
                  disabled={saving}
                  onChange={(event) => edit({ botName: event.target.value })}
                />
              </label>
              <span className={styles.hint}>{strings.sitesAssistantBotNameHint}</span>
            </div>

            <div className={styles.appearanceField}>
              <span className={styles.label}>{strings.sitesAssistantAvatarLabel}</span>
              <div className={styles.appearanceAvatarRow}>
                <span className={styles.themeSlotState}>
                  {fields.avatarBlobId !== null
                    ? strings.sitesThemeSet
                    : strings.sitesThemeNotSet}
                </span>
                <input
                  ref={fileInput}
                  type="file"
                  accept="image/*"
                  hidden
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    event.target.value = "";
                    if (file !== undefined) uploadAvatar(file);
                  }}
                />
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Upload size={14} />}
                  disabled={saving || uploading}
                  onClick={() => fileInput.current?.click()}
                >
                  {fields.avatarBlobId !== null
                    ? strings.sitesThemeReplace
                    : strings.sitesThemeUpload}
                </Button>
                {fields.avatarBlobId !== null && (
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<Trash2 size={14} />}
                    disabled={saving || uploading}
                    onClick={() => edit({ avatarBlobId: null })}
                  >
                    {strings.sitesThemeRemove}
                  </Button>
                )}
              </div>
              <span className={styles.hint}>{strings.sitesAssistantAvatarHint}</span>
              {uploadError !== null && (
                <span className={styles.publishError} role="alert">
                  {uploadError}
                </span>
              )}
            </div>

            <div className={styles.appearanceField}>
              <label className={styles.appearanceField}>
                <span className={styles.label}>{strings.sitesAssistantWelcomeLabel}</span>
                <textarea
                  className={styles.input}
                  rows={3}
                  value={fields.welcome}
                  maxLength={view.limits.welcomeChars}
                  disabled={saving}
                  onChange={(event) => edit({ welcome: event.target.value })}
                />
              </label>
              {fields.welcome.trim() === view.defaults.welcome.trim() && (
                <span className={styles.hint}>
                  {strings.sitesAssistantWelcomeDefaultNote}
                </span>
              )}
            </div>

            <fieldset className={styles.appearanceChoices}>
              <legend className={styles.label}>
                {strings.sitesAssistantQuestionsLegend}
              </legend>
              <span className={styles.hint}>{strings.sitesAssistantQuestionsHint}</span>
              {fields.questions.map((question, index) => (
                <label key={index} className={styles.appearanceField}>
                  <span className={styles.srOnly}>
                    {strings.sitesAssistantQuestionLabel(index + 1)}
                  </span>
                  <input
                    className={styles.input}
                    type="text"
                    value={question}
                    maxLength={view.limits.suggestedQuestionChars}
                    disabled={saving}
                    onChange={(event) => {
                      const questions = [...fields.questions];
                      questions[index] = event.target.value;
                      edit({ questions });
                    }}
                  />
                </label>
              ))}
              <div>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={<Sparkles size={14} />}
                  disabled={saving || suggesting}
                  onClick={() => void suggest()}
                >
                  {strings.sitesAssistantSuggestFromSite}
                </Button>
              </div>
              {suggestNote !== null && (
                <span className={styles.hint} role="status">
                  {suggestNote}
                </span>
              )}
            </fieldset>

            <ChoiceGroup
              legend={strings.sitesAssistantToneLegend}
              name="assistant-tone"
              value={fields.tone}
              disabled={saving}
              options={[
                { value: "formal", label: strings.sitesAssistantToneFormal },
                { value: "neutral", label: strings.sitesAssistantToneNeutral },
                { value: "warm", label: strings.sitesAssistantToneWarm },
              ]}
              onChange={(tone) => edit({ tone })}
            />

            <div className={styles.appearanceField}>
              <label className={styles.appearanceField}>
                <span className={styles.label}>{strings.sitesAssistantToneNoteLabel}</span>
                <textarea
                  className={styles.input}
                  rows={3}
                  value={fields.toneNote}
                  maxLength={view.limits.toneNoteChars}
                  disabled={saving}
                  onChange={(event) => edit({ toneNote: event.target.value })}
                />
              </label>
              <span className={styles.hint}>{strings.sitesAssistantToneNoteHint}</span>
            </div>

            <ChoiceGroup
              legend={strings.sitesAssistantCornerLegend}
              name="assistant-corner"
              value={fields.corner}
              disabled={saving}
              options={[
                { value: "right", label: strings.sitesAssistantCornerRight },
                { value: "left", label: strings.sitesAssistantCornerLeft },
              ]}
              onChange={(corner) => edit({ corner })}
            />

            <ChoiceGroup
              legend={strings.sitesAssistantIconLegend}
              name="assistant-icon"
              value={fields.icon}
              disabled={saving}
              options={[
                { value: "chat", label: strings.sitesAssistantIconChat },
                { value: "question", label: strings.sitesAssistantIconQuestion },
                { value: "sparkle", label: strings.sitesAssistantIconSparkle },
              ]}
              onChange={(icon) => edit({ icon })}
            />

            <ChoiceGroup
              legend={strings.sitesAssistantAccentLegend}
              hint={strings.sitesAssistantAccentHint}
              name="assistant-accent"
              value={fields.accent}
              disabled={saving}
              options={[
                { value: "primary", label: strings.sitesAssistantAccentPrimary },
                { value: "text", label: strings.sitesAssistantAccentText },
                { value: "surface", label: strings.sitesAssistantAccentSurface },
              ]}
              onChange={(accent) => edit({ accent })}
            />

            <label className={styles.assistantSwitch}>
              <input
                type="checkbox"
                checked={fields.autoOpen}
                disabled={saving}
                onChange={(event) => edit({ autoOpen: event.target.checked })}
              />
              <span>{strings.sitesAssistantAutoOpenLabel}</span>
            </label>
            <span className={styles.hint}>{strings.sitesAssistantAutoOpenHint}</span>

            <div className={styles.appearanceField}>
              <label className={styles.appearanceField}>
                <span className={styles.label}>{strings.sitesAssistantOfflineLabel}</span>
                <input
                  className={styles.input}
                  type="text"
                  value={fields.offline}
                  maxLength={view.limits.offlineMessageChars}
                  placeholder={view.defaults.offlineMessage}
                  disabled={saving}
                  onChange={(event) => edit({ offline: event.target.value })}
                />
              </label>
              <span className={styles.hint}>{strings.sitesAssistantOfflineHint}</span>
            </div>

            <div className={styles.languageControls}>
              <Button size="sm" disabled={busy} onClick={() => void save()}>
                {strings.sitesAssistantAppearanceSave}
              </Button>
              {saved && <span role="status">{strings.sitesAssistantSaved}</span>}
            </div>
            {saveError !== null && (
              <span className={styles.publishError} role="alert">
                {saveError}
              </span>
            )}
          </div>

          <div className={styles.appearancePreview}>
            <h3 className={styles.languageTitle}>{strings.sitesAssistantPreviewTitle}</h3>
            <p className={styles.hint}>{strings.sitesAssistantPreviewHint}</p>
            {/* Fully sandboxed: the preview document carries no script by
                design — it is a picture of the widget, not a live chat. */}
            <iframe
              className={styles.appearancePreviewFrame}
              title={strings.sitesAssistantPreviewFrameTitle}
              sandbox=""
              srcDoc={previewHtml ?? ""}
            />
            {previewError !== null && (
              <span className={styles.publishError} role="alert">
                {previewError}
              </span>
            )}

            <section
              className={styles.appearanceA11y}
              aria-labelledby="assistant-a11y-title"
            >
              <h3 id="assistant-a11y-title" className={styles.languageTitle}>
                {strings.sitesAssistantA11yTitle}
              </h3>
              <ul>
                <li>
                  {contrast !== null
                    ? strings.sitesAssistantA11yContrast(contrast.toLocaleString())
                    : strings.sitesAssistantA11yContrastGuarantee}
                </li>
                <li>{strings.sitesAssistantA11yKeyboard}</li>
                <li>{strings.sitesAssistantA11yAvatar}</li>
              </ul>
            </section>
          </div>
        </div>
      )}
    </section>
  );
}
