// The site theme dialog: direct reusable brand-color controls and the
// logo/favicon uploads. A shipped preset remains the typography foundation,
// but the person does not have to choose between decorative preset cards.
// Applying PUTs the full envelope through
// the server's theme gate — the dialog re-states no rules, and a 422 shows
// the server's own sentence. Uploads go through Drive (`driveUploadBlob`), so
// the image lands as a referenced, user-visible Drive file whose blob id the
// theme points at.
import { useEffect, useRef, useState } from "react";
import { Palette, Trash2, Upload } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { Button, IconButton, Spinner } from "../ds";
import { readBrandKit } from "../branding/repository";
import { sitesMessage, useSitesApi } from "./api";
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
  brandSource: "flex flex-wrap items-center justify-between gap-4 rounded-2xl border border-subtle bg-surface-muted p-4",
  brandSourceText: "min-w-0 flex-1",
  brandSourceTitle: "font-semibold text-primary",
  brandSourceHint: "mt-1 text-sm leading-5 text-secondary",
  brandSwatches: "flex items-center -space-x-1.5",
  brandSwatch: "size-8 rounded-full border-2 border-surface shadow-sm",
  assets: "mt-6 grid gap-3 border-t border-subtle pt-2 sm:grid-cols-2",
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
  const navigate = useNavigate();
  const [presets, setPresets] = useState<ThemePreset[] | null>(null);
  const [preset, setPreset] = useState<string | null>(null);
  const [logo, setLogo] = useState<string | null>(null);
  const [favicon, setFavicon] = useState<string | null>(null);
  const [colors, setColors] = useState<BrandColors | null>(null);
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
        const base = site.theme.colors ?? (selected === undefined ? null : presetColors(selected));
        if (base !== null) {
          const brand = readBrandKit();
          setColors({
            ...base,
            accent_1: brand.primary.value,
            accent_2: brand.secondary?.value ?? base.accent_2,
            accent_3: brand.supporting[0]?.value ?? base.accent_3,
            accent_4: brand.supporting[1]?.value ?? base.accent_4,
            accent_5: brand.supporting[2]?.value ?? base.accent_5,
          });
        }
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
      colors: colors ?? undefined,
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
    && Object.values(colors).every((value) => /^#[0-9A-F]{6}$/i.test(value));

  return (
    <DialogFrame
      Icon={Palette}
      title={strings.sitesThemeTitle}
      subtitle={strings.sitesThemeSubtitle}
      error={error}
      busy={busy}
      canSubmit={preset !== null && colorsValid}
      submitLabel={strings.sitesThemeApply}
      wide
      onClose={onClose}
      onSubmit={() => void apply()}
    >
      {loading && <Spinner size={18} />}
      {loadError !== null && <ErrorBanner message={loadError} />}
      {presets !== null && (
        <>
          {colors !== null && (
            <section className={styles.brandSource}>
              <div className={styles.brandSourceText}>
                <h3 className={styles.brandSourceTitle}>{strings.brandingAccentsTitle}</h3>
                <p className={styles.brandSourceHint}>{strings.sitesThemeBrandManaged}</p>
              </div>
              <div className={styles.brandSwatches} aria-hidden="true">
                {[colors.accent_1, colors.accent_2, colors.accent_3, colors.accent_4, colors.accent_5]
                  .map((value, index) => (
                    <span key={index} className={styles.brandSwatch} style={{ background: value }} />
                  ))}
              </div>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => {
                  onClose();
                  navigate("/branding");
                }}
              >
                {strings.moduleBranding}
              </Button>
            </section>
          )}
          <div className={styles.assets}>
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
          </div>
        </>
      )}
    </DialogFrame>
  );
}
