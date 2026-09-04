// The site theme dialog: direct reusable brand-color controls and the
// logo/favicon uploads. A shipped preset remains the typography foundation,
// but the person does not have to choose between decorative preset cards.
// Applying PUTs the full envelope through
// the server's theme gate — the dialog re-states no rules, and a 422 shows
// the server's own sentence. Uploads are registered through Sites in the
// website's source-linked Drive Identity folder, while the theme points at the
// underlying blob id.
import { useEffect, useRef, useState } from "react";
import type { DragEvent } from "react";
import { Palette, Trash2, Upload } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { Badge, Button, IconButton, Spinner } from "../ds";
import { FieldHelp } from "../branding/FieldHelp";
import { readBrandKit } from "../branding/repository";
import { sitesMessage, useSitesApi } from "./api";
import { DialogFrame, ErrorBanner } from "./parts";
import type { BrandColors, ThemeEnvelope, ThemePreset } from "./types";

const styles = {
  themeSlot: "mt-4 rounded-2xl border border-subtle bg-surface p-5 shadow-xs",
  themeSlotHeader: "flex min-w-0 items-center justify-between gap-3",
  themeSlotTitle: "flex min-w-0 items-center gap-1",
  label: "truncate font-semibold text-primary",
  themeSlotBody: "mt-4 flex min-w-0 items-center gap-4",
  themeSlotActions: "ml-auto flex shrink-0 items-center gap-1",
  themeDropzone:
    "mt-4 flex min-h-36 w-full flex-col items-center justify-center rounded-2xl border border-dashed border-default bg-surface-muted/45 px-6 py-5 text-center transition-[border-color,background-color,box-shadow] hover:border-accent/45 hover:bg-accent-soft/25 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25 disabled:cursor-wait disabled:opacity-60",
  themeDropIcon:
    "mb-3 grid size-11 place-items-center rounded-xl bg-surface text-accent shadow-sm ring-1 ring-border-subtle",
  themeDropTitle: "text-sm font-semibold text-primary",
  themeDropHint: "mt-1 text-xs leading-5 text-secondary",
  brandSource:
    "flex flex-wrap items-center justify-between gap-4 rounded-2xl border border-subtle bg-surface-muted p-4",
  brandSourceText: "min-w-0 flex-1",
  brandSourceTitle: "font-semibold text-primary",
  brandSourceHint: "mt-1 text-sm leading-5 text-secondary",
  brandSwatches: "flex items-center -space-x-1.5",
  brandSwatch: "size-8 rounded-full border-2 border-surface shadow-sm",
  assets: "mt-6 grid gap-3 border-t border-subtle pt-2 sm:grid-cols-2",
} as const;

/** The version this form writes; the server refuses anything else. */
const THEME_SCHEMA_VERSION = 1;

/** Resolve a stored Drive blob into a browser-safe preview URL. The URL is
 * temporary and must be revoked whenever the asset changes or the dialog
 * closes, otherwise repeatedly replacing a logo leaks the downloaded bytes. */
function useImagePreview(
  jmap: ReturnType<typeof useJmapClient>,
  blobId: string | null,
  name: string,
  localFile: File | null,
): string | null {
  const client = useRef(jmap);
  client.current = jmap;
  const [preview, setPreview] = useState<string | null>(null);

  useEffect(() => {
    let stale = false;
    let objectUrl: string | null = null;
    setPreview(null);
    if (blobId === null) return undefined;
    if (localFile !== null) {
      objectUrl = URL.createObjectURL(localFile);
      setPreview(objectUrl);
      return () => {
        if (objectUrl !== null) URL.revokeObjectURL(objectUrl);
      };
    }
    void (async () => {
      try {
        const blob = await client.current.downloadAttachment(blobId, name);
        if (stale) return;
        objectUrl = URL.createObjectURL(blob);
        setPreview(objectUrl);
      } catch {
        // The stored image remains valid even if this one preview cannot be
        // downloaded; upload/replace/remove must remain available.
      }
    })();
    return () => {
      stale = true;
      if (objectUrl !== null) URL.revokeObjectURL(objectUrl);
    };
  }, [blobId, localFile, name]);

  return preview;
}

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
  preview,
  compactPreview,
  busy,
  onUpload,
  onRemove,
}: {
  label: string;
  hint: string;
  blobId: string | null;
  preview: string | null;
  compactPreview?: boolean;
  busy: boolean;
  onUpload: (file: File) => void;
  onRemove: () => void;
}) {
  const fileInput = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);

  function keepFile(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragging(false);
    const file = event.dataTransfer.files[0];
    if (file !== undefined) onUpload(file);
  }

  return (
    <div
      role="group"
      aria-label={label}
      className={`${styles.themeSlot} ${dragging ? "border-accent bg-accent-soft/35 ring-2 ring-accent/15" : ""}`}
      onDragEnter={(event) => {
        event.preventDefault();
        if (!busy) setDragging(true);
      }}
      onDragOver={(event) => event.preventDefault()}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          setDragging(false);
        }
      }}
      onDrop={(event) => {
        if (busy) {
          event.preventDefault();
          return;
        }
        keepFile(event);
      }}
    >
      <div className={styles.themeSlotHeader}>
        <div className={styles.themeSlotTitle}>
          <span className={styles.label}>{label}</span>
          <FieldHelp title={label}>{hint}</FieldHelp>
        </div>
        <Badge tone={blobId !== null ? "success" : "neutral"}>
          {blobId !== null ? strings.sitesThemeSet : strings.sitesThemeNotSet}
        </Badge>
      </div>
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
      {blobId === null ? (
        <button
          type="button"
          className={styles.themeDropzone}
          disabled={busy}
          onClick={() => fileInput.current?.click()}
        >
          <span className={styles.themeDropIcon}>
            <Upload size={19} aria-hidden="true" />
          </span>
          <span className={styles.themeDropTitle}>
            {dragging ? strings.sitesThemeDropNow : strings.sitesThemeDropTitle}
          </span>
          <span className={styles.themeDropHint}>
            {strings.sitesThemeDropBrowse}
          </span>
        </button>
      ) : (
        <div className={styles.themeSlotBody}>
          <span
            className={
              compactPreview
                ? "grid size-16 shrink-0 place-items-center overflow-hidden rounded-2xl border border-subtle bg-surface-muted p-3"
                : "grid h-16 w-24 shrink-0 place-items-center overflow-hidden rounded-2xl border border-subtle bg-surface-muted p-3"
            }
          >
            {preview !== null ? (
              <img
                src={preview}
                alt={label}
                className="max-h-full max-w-full object-contain"
              />
            ) : (
              <Spinner size={18} />
            )}
          </span>
          <div className={styles.themeSlotActions}>
            <Button
              variant="secondary"
              size="sm"
              icon={<Upload size={14} />}
              disabled={busy}
              onClick={() => fileInput.current?.click()}
            >
              {strings.sitesThemeReplace}
            </Button>
            <IconButton
              size="sm"
              tone="danger"
              label={strings.sitesThemeRemove}
              icon={<Trash2 size={14} />}
              disabled={busy}
              onClick={onRemove}
            />
          </div>
        </div>
      )}
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
  const [logoFile, setLogoFile] = useState<File | null>(null);
  const [faviconFile, setFaviconFile] = useState<File | null>(null);
  const [colors, setColors] = useState<BrandColors | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const logoPreview = useImagePreview(jmap, logo, "site-logo", logoFile);
  const faviconPreview = useImagePreview(
    jmap,
    favicon,
    "site-favicon",
    faviconFile,
  );

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
        setLogoFile(null);
        setFaviconFile(null);
        const selected = shipped.find(
          (item) => item.id === (site.theme.preset ?? shipped[0]?.id),
        );
        const base =
          site.theme.colors ??
          (selected === undefined ? null : presetColors(selected));
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

  function upload(
    setBlobId: (blobId: string) => void,
    setLocalFile: (file: File | null) => void,
  ) {
    return (file: File) => {
      if (!file.type.startsWith("image/")) {
        setError(strings.sitesUploadFailed);
        return;
      }
      setBusy(true);
      setError(null);
      void (async () => {
        try {
          const { blobId } = await jmap.uploadFile(file);
          await api.attachIdentityImage(siteId, {
            blobId,
            filename: file.name,
          });
          setLocalFile(file);
          setBlobId(blobId);
        } catch {
          setError(strings.sitesUploadFailed);
        } finally {
          setBusy(false);
        }
      })();
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
  const colorsValid =
    colors !== null &&
    Object.values(colors).every((value) => /^#[0-9A-F]{6}$/i.test(value));

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
                <h3 className={styles.brandSourceTitle}>
                  {strings.brandingAccentsTitle}
                </h3>
                <p className={styles.brandSourceHint}>
                  {strings.sitesThemeBrandManaged}
                </p>
              </div>
              <div className={styles.brandSwatches} aria-hidden="true">
                {[
                  colors.accent_1,
                  colors.accent_2,
                  colors.accent_3,
                  colors.accent_4,
                  colors.accent_5,
                ].map((value, index) => (
                  <span
                    key={index}
                    className={styles.brandSwatch}
                    style={{ background: value }}
                  />
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
              preview={logoPreview}
              busy={busy}
              onUpload={upload(setLogo, setLogoFile)}
              onRemove={() => {
                setLogo(null);
                setLogoFile(null);
              }}
            />
            <ImageSlot
              label={strings.sitesThemeFavicon}
              hint={strings.sitesThemeFaviconHint}
              blobId={favicon}
              preview={faviconPreview}
              compactPreview
              busy={busy}
              onUpload={upload(setFavicon, setFaviconFile)}
              onRemove={() => {
                setFavicon(null);
                setFaviconFile(null);
              }}
            />
          </div>
        </>
      )}
    </DialogFrame>
  );
}
