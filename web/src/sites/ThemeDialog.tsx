// The site theme dialog: the preset picker (each shipped palette as a swatch
// card) and the logo/favicon uploads. Applying PUTs the full envelope through
// the server's theme gate — the dialog re-states no rules, and a 422 shows
// the server's own sentence. Uploads go through Drive (`driveUploadBlob`), so
// the image lands as a referenced, user-visible Drive file whose blob id the
// theme points at.
import { useEffect, useRef, useState } from "react";
import { Palette, Trash2, Upload } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { Button, ColorPicker, IconButton, Spinner } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { contrastRatio } from "./accentContrast";
import { DialogFrame, ErrorBanner } from "./parts";
import type { BrandColors, ThemeEnvelope, ThemePreset } from "./types";

const styles = {
  themeSlot:
    "mt-4 flex flex-col gap-4 rounded-2xl border border-subtle bg-surface-muted p-4 sm:flex-row sm:items-center sm:justify-between",
  themeSlotText: "min-w-0",
  label: "block font-semibold text-primary",
  hint: "mt-1 block max-w-xl text-sm leading-5 text-secondary",
  themeSlotActions: "flex flex-wrap items-center gap-2",
  themeSlotState:
    "mr-1 inline-flex min-h-8 items-center rounded-full bg-surface px-3 text-xs font-semibold text-secondary",
  presetGrid: "grid gap-3 sm:grid-cols-2 lg:grid-cols-3",
  presetCard:
    "flex min-h-28 flex-col justify-between rounded-2xl border-2 p-5 text-left transition-[border-color,box-shadow] hover:border-accent hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2",
  presetCardActive: "ring-2 ring-accent ring-offset-2",
  presetName: "text-lg",
  presetSwatches: "mt-5 flex gap-2",
  presetSwatch: "size-7 rounded-full border border-black/10 shadow-sm",
  colorPanel: "mt-5 overflow-hidden rounded-2xl border border-subtle bg-surface",
  colorPanelHead: "flex flex-wrap items-start justify-between gap-3 border-b border-subtle px-5 py-4",
  colorPanelTitle: "font-semibold text-primary",
  colorPanelHint: "mt-1 max-w-2xl text-sm leading-5 text-secondary",
  colorGroup: "px-5 py-4 [&+&]:border-t [&+&]:border-subtle",
  colorGroupTitle: "mb-3 text-xs font-semibold uppercase tracking-wide text-secondary",
  colorGrid: "grid gap-3 sm:grid-cols-2",
  colorRow: "flex min-w-0 items-center gap-3 rounded-xl border border-subtle bg-surface-muted px-3 py-3",
  colorLabel: "flex min-w-0 flex-1 items-center justify-between gap-2 text-sm font-semibold text-primary",
  colorInput: "w-24 rounded-lg border border-subtle bg-surface px-2 py-1.5 font-mono text-xs uppercase text-primary",
  colorError: "px-5 pb-4 text-sm font-medium text-danger",
} as const;

/** The version this form writes; the server refuses anything else. */
const THEME_SCHEMA_VERSION = 1;

function presetColors(preset: ThemePreset): BrandColors {
  return {
    background: preset.palette.background.toUpperCase(),
    text: preset.palette.text.toUpperCase(),
    border: preset.palette.border.toUpperCase(),
    accent_1: preset.palette.primary.toUpperCase(),
    accent_2: preset.palette.mutedText.toUpperCase(),
    accent_3: preset.palette.surface.toUpperCase(),
    accent_4: preset.palette.text.toUpperCase(),
    accent_5: preset.palette.background.toUpperCase(),
  };
}

const colorFields: ReadonlyArray<[keyof BrandColors, string]> = [
  ["background", strings.sitesThemeBackgroundColor],
  ["text", strings.sitesThemeTextColor],
  ["border", strings.sitesThemeBorderColor],
  ["accent_1", strings.sitesThemeAccentColor(1)],
  ["accent_2", strings.sitesThemeAccentColor(2)],
  ["accent_3", strings.sitesThemeAccentColor(3)],
  ["accent_4", strings.sitesThemeAccentColor(4)],
  ["accent_5", strings.sitesThemeAccentColor(5)],
];

/** One image slot of the theme (logo or favicon): its current blob id, an
 *  upload that replaces it, and a remove that clears it. */
function ImageSlot({
  label,
  hint,
  blobId,
  busy,
  onUpload,
  onRemove,
}: {
  label: string;
  hint: string;
  blobId: string | null;
  busy: boolean;
  onUpload: (file: File) => void;
  onRemove: () => void;
}) {
  const fileInput = useRef<HTMLInputElement>(null);
  return (
    <div className={styles.themeSlot}>
      <div className={styles.themeSlotText}>
        <span className={styles.label}>{label}</span>
        <span className={styles.hint}>{hint}</span>
      </div>
      <div className={styles.themeSlotActions}>
        <span className={styles.themeSlotState}>
          {blobId !== null ? strings.sitesThemeSet : strings.sitesThemeNotSet}
        </span>
        <input
          ref={fileInput}
          type="file"
          accept="image/*"
          hidden
          onChange={(e) => {
            const file = e.target.files?.[0];
            // Allow re-picking the same file after a remove.
            e.target.value = "";
            if (file !== undefined) onUpload(file);
          }}
        />
        <Button
          variant="ghost"
          size="sm"
          icon={<Upload size={14} />}
          disabled={busy}
          onClick={() => fileInput.current?.click()}
        >
          {blobId !== null
            ? strings.sitesThemeReplace
            : strings.sitesThemeUpload}
        </Button>
        {blobId !== null && (
          <IconButton
            size="sm"
            label={strings.sitesThemeRemove}
            icon={<Trash2 size={14} />}
            disabled={busy}
            onClick={onRemove}
          />
        )}
      </div>
    </div>
  );
}

/** The theme dialog. Loads the site's stored theme and the shipped presets
 *  itself, so it can mount from any screen that knows a site id; `onApplied`
 *  fires after the server accepted the envelope (the editor refreshes its
 *  preview on it). */
export function ThemeDialog({
  siteId,
  onClose,
  onApplied,
}: {
  siteId: string;
  onClose: () => void;
  onApplied: () => void;
}) {
  const api = useSitesApi();
  const jmap = useJmapClient();
  const [presets, setPresets] = useState<ThemePreset[] | null>(null);
  const [preset, setPreset] = useState<string | null>(null);
  const [logo, setLogo] = useState<string | null>(null);
  const [favicon, setFavicon] = useState<string | null>(null);
  const [colors, setColors] = useState<BrandColors | null>(null);
  const [customColors, setCustomColors] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let stale = false;
    Promise.all([api.site(siteId), api.themePresets()]).then(
      ([site, shipped]) => {
        if (stale) return;
        setPresets(shipped);
        // A pristine site stores `{}`: the default is the first shipped preset.
        setPreset(site.theme.preset ?? shipped[0]?.id ?? null);
        setLogo(site.theme.logo ?? null);
        setFavicon(site.theme.favicon ?? null);
        const selected = shipped.find((item) => item.id === (site.theme.preset ?? shipped[0]?.id));
        setColors(site.theme.colors ?? (selected === undefined ? null : presetColors(selected)));
        setCustomColors(site.theme.colors !== undefined);
      },
      (err: unknown) => {
        if (!stale)
          setLoadError(sitesMessage(err, strings.sitesThemeLoadFailed));
      },
    );
    return () => {
      stale = true;
    };
  }, [api, siteId]);

  function upload(set: (blobId: string) => void) {
    return (file: File) => {
      setBusy(true);
      setError(null);
      jmap.driveUploadBlob(null, null, file).then(
        ({ blobId }) => {
          set(blobId);
          setBusy(false);
        },
        () => {
          setError(strings.sitesUploadFailed);
          setBusy(false);
        },
      );
    };
  }

  async function apply() {
    if (preset === null) return;
    setBusy(true);
    setError(null);
    const envelope: ThemeEnvelope = {
      schema_version: THEME_SCHEMA_VERSION,
      preset,
      logo: logo ?? undefined,
      favicon: favicon ?? undefined,
      colors: customColors ? colors ?? undefined : undefined,
    };
    try {
      await api.setTheme(siteId, envelope);
      onApplied();
    } catch (err) {
      setError(sitesMessage(err, strings.sitesSaveFailed));
      setBusy(false);
    }
  }

  const loading = presets === null && loadError === null;
  const colorsValid = colors !== null
    && colorFields.every(([field]) => /^#[0-9A-F]{6}$/i.test(colors[field]))
    && (contrastRatio(colors.background, colors.text) ?? 0) >= 4.5;

  return (
    <DialogFrame
      Icon={Palette}
      title={strings.sitesThemeTitle}
      subtitle={strings.sitesThemeSubtitle}
      error={error}
      busy={busy}
      canSubmit={preset !== null && colorsValid}
      submitLabel={strings.sitesThemeApply}
      onClose={onClose}
      onSubmit={() => void apply()}
    >
      {loading && <Spinner size={18} />}
      {loadError !== null && <ErrorBanner message={loadError} />}
      {presets !== null && (
        <>
          <div
            className={styles.presetGrid}
            role="radiogroup"
            aria-label={strings.sitesThemePresets}
          >
            {presets.map((p) => (
              <button
                key={p.id}
                type="button"
                role="radio"
                aria-checked={p.id === preset}
                className={
                  p.id === preset
                    ? `${styles.presetCard} ${styles.presetCardActive}`
                    : styles.presetCard
                }
                style={{
                  background: p.palette.background,
                  borderColor: p.palette.border,
                }}
                onClick={() => {
                  setPreset(p.id);
                  setColors(presetColors(p));
                  setCustomColors(false);
                }}
              >
                <span
                  className={styles.presetName}
                  style={{
                    color: p.palette.text,
                    fontFamily: p.typography.headingFamily,
                    fontWeight: p.typography.headingWeight,
                  }}
                >
                  {p.name}
                </span>
                <span className={styles.presetSwatches} aria-hidden="true">
                  {[
                    p.palette.primary,
                    p.palette.surface,
                    p.palette.mutedText,
                  ].map((color, i) => (
                    <span
                      key={i}
                      className={styles.presetSwatch}
                      style={{ background: color }}
                    />
                  ))}
                </span>
              </button>
            ))}
          </div>
          {colors !== null && (
            <section className={styles.colorPanel}>
              <div className={styles.colorPanelHead}>
                <div>
                  <h3 className={styles.colorPanelTitle}>{strings.sitesThemeBrandColors}</h3>
                  <p className={styles.colorPanelHint}>{strings.sitesThemeBrandColorsHint}</p>
                </div>
                {customColors && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      const selected = presets.find((item) => item.id === preset);
                      if (selected !== undefined) setColors(presetColors(selected));
                      setCustomColors(false);
                    }}
                  >
                    {strings.sitesThemeResetColors}
                  </Button>
                )}
              </div>
              {[colorFields.slice(0, 3), colorFields.slice(3)].map((fields, group) => (
                <div key={group} className={styles.colorGroup}>
                  <h4 className={styles.colorGroupTitle}>
                    {group === 0 ? strings.sitesThemeBaseColors : strings.sitesThemeAccentColors}
                  </h4>
                  <div className={styles.colorGrid}>
                    {fields.map(([field, label]) => (
                      <div key={field} className={styles.colorRow}>
                        <ColorPicker
                          label={label}
                          value={colors[field]}
                          onChange={(value) => {
                            setColors({ ...colors, [field]: value.toUpperCase() });
                            setCustomColors(true);
                          }}
                        />
                        <label className={styles.colorLabel}>
                          {label}
                          <input
                            className={styles.colorInput}
                            aria-label={strings.sitesThemeHexValue(label)}
                            value={colors[field]}
                            maxLength={7}
                            onChange={(event) => {
                              setColors({ ...colors, [field]: event.target.value.toUpperCase() });
                              setCustomColors(true);
                            }}
                          />
                        </label>
                      </div>
                    ))}
                  </div>
                </div>
              ))}
              {!colorsValid && <p className={styles.colorError}>{strings.sitesThemeColorError}</p>}
            </section>
          )}
          <ImageSlot
            label={strings.sitesThemeLogo}
            hint={strings.sitesThemeLogoHint}
            blobId={logo}
            busy={busy}
            onUpload={upload(setLogo)}
            onRemove={() => setLogo(null)}
          />
          <ImageSlot
            label={strings.sitesThemeFavicon}
            hint={strings.sitesThemeFaviconHint}
            blobId={favicon}
            busy={busy}
            onUpload={upload(setFavicon)}
            onRemove={() => setFavicon(null)}
          />
        </>
      )}
    </DialogFrame>
  );
}
