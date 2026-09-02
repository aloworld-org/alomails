// The per-type prop forms — one small form per section kind, sharing the same
// few field primitives, in the module's dialog chrome. The form edits a
// draft (`sectionDrafts.ts`) and hands the wire section up on save; the
// SERVER rules on content (blank required text, bad hrefs, empty lists) and
// its 422 sentence is shown here verbatim, so there is exactly one copy of
// every rule.
import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import {
  Blocks,
  Check,
  ChevronDown,
  ChevronUp,
  ChevronRight,
  Image as ImageIcon,
  Link2,
  MoreHorizontal,
  MousePointerClick,
  Palette,
  PanelTop,
  PanelsTopLeft,
  Play,
  Plus,
  Sparkles,
  Settings2,
  Trash2,
  Type,
} from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { strings } from "../i18n";
import {
  Button,
  Card,
  ChoicePicker,
  cx,
  IconButton,
  Input,
  Menu,
  moduleNavigationItemClassName,
  Spinner,
} from "../ds";
import { readBrandKit } from "../branding/repository";
import { kindDescription, kindLabel } from "./sectionInfo";
import { contrastRatio } from "./accentContrast";
import {
  blankFaqItem,
  blankFeature,
  blankImage,
  blankLink,
  blankMember,
  blankTestimonial,
  blankTier,
  DEFAULT_SECTION_PRESENTATION,
  toDraft,
  toSection,
} from "./sectionDrafts";
import type {
  BookingDraft,
  CatalogDraft,
  ContactFormDraft,
  CollectionDraft,
  CtaDraft,
  FaqDraft,
  FeaturesDraft,
  FooterDraft,
  GalleryDraft,
  HeroDraft,
  NavDraft,
  PricingDraft,
  PresentableDraft,
  SectionDraft,
  TeamDraft,
  TestimonialsDraft,
  TextImageDraft,
  ShopDraft,
  TicketsDraft,
  TransitionDraft,
} from "./sectionDrafts";
import type {
  Section,
  SectionKind,
  SectionLink,
  ThemeColorRole,
  SectionPresentation,
} from "./sections";
import type {
  BrandColors,
  SiteCopyAction,
  SiteEditEnvelope,
  SitePage,
  ThemePreset,
} from "./types";
import type {
  SiteBooking,
  SiteCatalog,
  SiteCatalogCategory,
  SiteCollection,
} from "./types";
import { sitesMessage, useSitesApi } from "./api";
import { CopyContext, useCopyContext } from "./copyContext";
import type { CopyContextValue } from "./copyContext";
import { CustomCodeFields } from "./CustomCodeFields";
import { ImageFields } from "./ImageFields";
import { DialogFrame, Field } from "./parts";
import styles from "./SitesModule.module.css";

// ---- field primitives -------------------------------------------------------

function CopyTools({ pointer, value }: { pointer: string; value: string }) {
  const context = useCopyContext();
  const api = useSitesApi();
  const [open, setOpen] = useState(false);
  const [tone, setTone] = useState("");
  const [proposal, setProposal] = useState<SiteEditEnvelope | null>(null);
  const [after, setAfter] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (context === null || value.trim() === "") return null;

  async function propose(action: SiteCopyAction) {
    if (context === null) return;
    setBusy(true);
    setError(null);
    try {
      const prepared = await api.proposePageCopyEdit(
        context.siteId,
        context.pageId,
        {
          target: context.target,
          pointer,
          action,
          ...(action === "tone" ? { tone: tone.trim() } : {}),
        },
      );
      const operation = prepared.proposal.operations[0];
      if (operation?.op !== "rewrite_copy")
        throw new Error(strings.sitesAiCopyFailed);
      setProposal(prepared.proposal);
      setAfter(operation.text);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesAiCopyFailed));
    } finally {
      setBusy(false);
    }
  }

  async function apply() {
    if (context === null || proposal === null) return;
    setBusy(true);
    setError(null);
    try {
      context.onApplied(
        await api.applyPageEdit(context.siteId, context.pageId, proposal),
      );
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesAiApplyFailed));
      setBusy(false);
    }
  }

  return (
    <div className={styles.copyTools}>
      <Button
        variant="ghost"
        icon={<Sparkles size={16} />}
        aria-expanded={open}
        onClick={() => {
          setOpen((shown) => !shown);
          setProposal(null);
          setError(null);
        }}
      >
        {strings.sitesAiImproveCopy}
      </Button>
      {open && proposal === null && (
        <div
          className={styles.copyChoices}
          aria-label={strings.sitesAiCopyActions}
        >
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => void propose("rewrite")}
          >
            {strings.sitesAiRewrite}
          </Button>
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => void propose("shorter")}
          >
            {strings.sitesAiShorter}
          </Button>
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => void propose("longer")}
          >
            {strings.sitesAiLonger}
          </Button>
          <div className={styles.copyTone}>
            <input
              className={styles.input}
              value={tone}
              maxLength={60}
              aria-label={strings.sitesAiTone}
              placeholder={strings.sitesAiTonePlaceholder}
              onChange={(event) => setTone(event.target.value)}
            />
            <Button
              variant="ghost"
              disabled={busy || tone.trim() === ""}
              onClick={() => void propose("tone")}
            >
              {strings.sitesAiUseTone}
            </Button>
          </div>
        </div>
      )}
      {open && proposal !== null && (
        <div className={styles.copyProposal} aria-live="polite">
          <div>
            <span>{strings.sitesAiCopyBefore}</span>
            <p>{value}</p>
          </div>
          <div>
            <span>{strings.sitesAiCopyAfter}</span>
            <p>{after}</p>
          </div>
          <p className={styles.copyProposalHint}>
            {strings.sitesAiPreviewHint}
          </p>
          <div className={styles.copyProposalActions}>
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() => {
                setProposal(null);
                setAfter("");
              }}
            >
              {strings.sitesAiDiscard}
            </Button>
            <Button disabled={busy} onClick={() => void apply()}>
              {busy ? strings.sitesAiApplying : strings.sitesAiApprove}
            </Button>
          </div>
        </div>
      )}
      {error !== null && (
        <p className={styles.aiEditError} role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

function TextField({
  label,
  value,
  onChange,
  hint,
  mono = false,
  autoFocus = false,
  copyPointer,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  hint?: string;
  mono?: boolean;
  autoFocus?: boolean;
  copyPointer?: string;
}) {
  return (
    <Field label={label} hint={hint}>
      <input
        className={mono ? `${styles.input} ${styles.mono}` : styles.input}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        autoFocus={autoFocus}
        {...(mono
          ? { autoCapitalize: "none", autoCorrect: "off", spellCheck: false }
          : {})}
      />
      {copyPointer !== undefined && (
        <CopyTools pointer={copyPointer} value={value} />
      )}
    </Field>
  );
}

function LongTextField({
  label,
  value,
  onChange,
  hint,
  copyPointer,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  hint?: string;
  copyPointer?: string;
}) {
  return (
    <Field label={label} hint={hint}>
      <textarea
        className={`${styles.input} ${styles.textarea}`}
        value={value}
        rows={4}
        onChange={(e) => onChange(e.target.value)}
      />
      {copyPointer !== undefined && (
        <CopyTools pointer={copyPointer} value={value} />
      )}
    </Field>
  );
}

function CheckField({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className={styles.toggle}>
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      {label}
    </label>
  );
}

/** A link's two inputs. Both blank means "no link" for optional slots. */
function LinkFields({
  legend,
  value,
  onChange,
  bare = false,
}: {
  legend?: string;
  value: SectionLink;
  onChange: (patch: Partial<SectionLink>) => void;
  bare?: boolean;
}) {
  return (
    <fieldset className={bare ? "m-0 min-w-0 border-0 p-0" : styles.subGroup}>
      {legend !== undefined && (
        <legend
          className={
            bare ? "mb-3 text-sm font-semibold text-primary" : styles.subLegend
          }
        >
          {legend}
        </legend>
      )}
      <div className={styles.fieldRow}>
        <Field label={strings.sitesFieldLinkLabel}>
          <input
            className={styles.input}
            value={value.label}
            onChange={(e) => onChange({ label: e.target.value })}
          />
        </Field>
        <Field label={strings.sitesFieldLinkHref}>
          <input
            className={`${styles.input} ${styles.mono}`}
            value={value.href}
            onChange={(e) => onChange({ href: e.target.value })}
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
          />
        </Field>
      </div>
    </fieldset>
  );
}

/** The repeating-entries editor every list prop shares: numbered groups with
 *  a remove button each, and one add button at the end. Order is the order
 *  on the page; entries left blank are dropped on save, not sent as errors. */
function ItemsEditor<T extends object>({
  addLabel,
  items,
  onChange,
  blank,
  render,
}: {
  addLabel: string;
  items: T[];
  onChange: (items: T[]) => void;
  blank: () => T;
  render: (
    item: T,
    update: (patch: Partial<T>) => void,
    index: number,
  ) => ReactNode;
}) {
  const update = (index: number) => (patch: Partial<T>) => {
    onChange(
      items.map((item, i) => (i === index ? { ...item, ...patch } : item)),
    );
  };
  return (
    <div className={styles.itemsEditor}>
      {items.map((item, i) => (
        // Entries have no identity of their own — the position is the key.
        <div key={i} className={styles.itemGroup}>
          <div className={styles.itemGroupHead}>
            <span className={styles.itemGroupName}>
              {strings.sitesItemN(i + 1)}
            </span>
            <IconButton
              size="sm"
              label={strings.sitesRemoveItem}
              icon={<Trash2 size={14} />}
              onClick={() => onChange(items.filter((_, j) => j !== i))}
            />
          </div>
          {render(item, update(i), i)}
        </div>
      ))}
      <Button
        variant="ghost"
        size="sm"
        icon={<Plus size={14} />}
        onClick={() => onChange([...items, blank()])}
      >
        {addLabel}
      </Button>
    </div>
  );
}

// ---- the per-type field bodies ----------------------------------------------

type Change = (draft: SectionDraft) => void;

const NAV_DEFAULT_ROLES = {
  background: "background",
  text: "text",
  hover: "accent_1",
} as const;

const HERO_DEFAULT_ROLES = {
  background: "background",
  primary_button: "accent_1",
  primary_button_hover: "accent_2",
  secondary_button: "accent_3",
  secondary_button_hover: "accent_1",
} as const;

const HERO_FALLBACK_COLORS: BrandColors = {
  background: "#ffffff",
  text: "#17212b",
  border: "#dde3e9",
  accent_1: "#1d4ed8",
  accent_2: "#4c5866",
  accent_3: "#f2f5f8",
  accent_4: "#17212b",
  accent_5: "#ffffff",
};

function themeColors(preset: ThemePreset, custom?: BrandColors): BrandColors {
  return (
    custom ?? {
      background: preset.palette.background,
      text: preset.palette.text,
      border: preset.palette.border,
      accent_1: preset.palette.primary,
      accent_2: preset.palette.mutedText,
      accent_3: preset.palette.surface,
      accent_4: preset.palette.text,
      accent_5: preset.palette.background,
    }
  );
}

function roleColor(colors: BrandColors, role: ThemeColorRole): string {
  return colors[role];
}

function HeroColorSwatches({
  label,
  value,
  options,
  automatic = false,
  onChange,
}: {
  label: string;
  value: string;
  options: Array<{ value: string; label: string; swatch?: string | undefined }>;
  automatic?: boolean;
  onChange: (value: string) => void;
}) {
  const choices = automatic
    ? [{ value: "auto", label: strings.sitesHeroAutomaticContrast }, ...options]
    : options;
  return (
    <div className="grid gap-2">
      <span className="text-sm font-semibold text-primary">{label}</span>
      <div className="flex min-h-11 flex-wrap items-center gap-2" role="radiogroup" aria-label={label}>
        {choices.map((option) => {
          const selected = option.value === value;
          return (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={selected}
              aria-label={`${label}: ${option.label}`}
              title={option.label}
              className={cx(
                "relative grid size-9 place-items-center rounded-full !border bg-surface !p-0 shadow-sm transition-[border-color,box-shadow,transform] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30",
                selected
                  ? "!border-accent ring-2 ring-accent/20"
                  : "!border-default hover:scale-105 hover:!border-accent/50",
              )}
              onClick={() => onChange(option.value)}
            >
              <span
                className="size-6 rounded-full border border-black/10"
                style={{
                  background:
                    option.value === "auto"
                      ? "linear-gradient(135deg,#17212b 0 50%,#ffffff 50% 100%)"
                      : option.swatch,
                }}
                aria-hidden="true"
              />
              {selected && (
                <Check
                  className="absolute -right-1 -top-1 size-4 rounded-full bg-accent p-0.5 text-on-accent"
                  aria-hidden="true"
                />
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function sectionTargets(page: SitePage, sections: Section[]) {
  const occurrences = new Map<string, number>();
  return sections.flatMap((section) => {
    if (section.type === "nav" || section.type === "footer") return [];
    const occurrence = (occurrences.get(section.type) ?? 0) + 1;
    occurrences.set(section.type, occurrence);
    const anchor =
      occurrence === 1 ? section.type : `${section.type}-${occurrence}`;
    const path = page.home ? "/" : `/${page.slug}`;
    return [
      {
        href: `${path}#${anchor}`,
        label: `${page.title} · ${kindLabel(section.type)}`,
        defaultLabel: kindLabel(section.type),
      },
    ];
  });
}

function NavFields({
  draft,
  onChange,
  currentPage,
  currentSections,
}: {
  draft: NavDraft;
  onChange: Change;
  currentPage?: SitePage | undefined;
  currentSections: Section[];
}) {
  const { siteId = "", pageId: routePageId = "" } = useParams();
  const pageId = routePageId || currentPage?.id || "";
  const api = useSitesApi();
  const workspaceBrand = readBrandKit();
  const [pages, setPages] = useState<SitePage[]>([]);
  const [pagesLoading, setPagesLoading] = useState(siteId !== "");
  const [pagesFailed, setPagesFailed] = useState(false);
  const [pageSections, setPageSections] = useState<Record<string, Section[]>>(
    currentPage === undefined ? {} : { [currentPage.id]: currentSections },
  );
  const [openLink, setOpenLink] = useState<number | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [actionOpen, setActionOpen] = useState(
    draft.cta.label.trim() !== "" || draft.cta.href.trim() !== "",
  );
  const [appearanceOpen, setAppearanceOpen] = useState(false);
  const [brandColors, setBrandColors] = useState<BrandColors | null>(null);

  useEffect(() => {
    if (siteId === "") return;
    let current = true;
    setPagesLoading(true);
    setPagesFailed(false);
    void api
      .pages(siteId)
      .then((loaded) => {
        if (!current) return;
        setPages(loaded);
        const missing = loaded.filter((page) => page.id !== pageId);
        void Promise.allSettled(
          missing.map((page) => api.page(siteId, page.id)),
        ).then((results) => {
          if (!current) return;
          const loadedSections: Record<string, Section[]> =
            currentPage === undefined
              ? {}
              : { [currentPage.id]: currentSections };
          results.forEach((result, index) => {
            const page = missing[index];
            if (result.status === "fulfilled" && page !== undefined) {
              const sections: unknown = result.value.sections?.sections;
              if (Array.isArray(sections))
                loadedSections[page.id] = sections as Section[];
            }
          });
          setPageSections(loadedSections);
        });
      })
      .catch(() => {
        if (current) setPagesFailed(true);
      })
      .finally(() => {
        if (current) setPagesLoading(false);
      });
    return () => {
      current = false;
    };
  }, [api, currentPage, currentSections, pageId, siteId]);

  useEffect(() => {
    if (siteId === "") return;
    let current = true;
    void Promise.all([api.site(siteId), api.themePresets()])
      .then(([site, presets]) => {
        if (!current) return;
        const preset = presets.find(
          (item) => item.id === (site.theme.preset ?? presets[0]?.id),
        );
        if (preset !== undefined)
          setBrandColors(themeColors(preset, site.theme.colors));
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [api, siteId]);

  const pagePath = (page: SitePage) => (page.home ? "/" : `/${page.slug}`);
  const linkedTargets = new Set(draft.links.map((link) => link.href.trim()));
  const missingPages = pages.filter(
    (page) => !linkedTargets.has(pagePath(page)),
  );
  const destinations = pages.flatMap((page) =>
    sectionTargets(page, pageSections[page.id] ?? []),
  );
  const destinationOptions = [
    { value: "custom", label: strings.sitesNavCustomTarget },
    ...pages.map((page) => ({
      value: pagePath(page),
      label: `${page.title} · ${pagePath(page)}`,
    })),
    ...destinations.map((target) => ({
      value: target.href,
      label: target.label,
    })),
  ];
  const appearanceOptions = [
    { value: "background", label: strings.sitesThemeBackgroundColor, swatch: brandColors?.background },
    { value: "text", label: strings.sitesThemeTextColor, swatch: brandColors?.text },
    { value: "border", label: strings.sitesThemeBorderColor, swatch: brandColors?.border },
    { value: "accent_1", label: strings.sitesThemeAccentColor(1), swatch: brandColors?.accent_1 },
    ...(workspaceBrand.secondary === null
      ? []
      : [{ value: "accent_2", label: strings.sitesThemeAccentColor(2), swatch: brandColors?.accent_2 }]),
    ...workspaceBrand.supporting.map((color, index) => ({
      value: `accent_${index + 3}`,
      label: color.name,
      swatch: brandColors?.[`accent_${index + 3}` as keyof BrandColors],
    })),
  ];
  const reorder = (from: number, to: number) => {
    if (from === to || to < 0 || to >= draft.links.length) return;
    const links = [...draft.links];
    const [moved] = links.splice(from, 1);
    if (moved === undefined) return;
    links.splice(to, 0, moved);
    onChange({ ...draft, links });
  };
  const addSitePages = () => {
    const typed = draft.links.filter(
      (link) => link.label.trim() !== "" || link.href.trim() !== "",
    );
    const existing = new Set(typed.map((link) => link.href.trim()));
    const added = pages
      .filter((page) => !existing.has(pagePath(page)))
      .map((page) => ({ label: page.title, href: pagePath(page) }));
    onChange({ ...draft, links: [...typed, ...added] });
  };

  const selectedAppearance = draft.appearance ?? NAV_DEFAULT_ROLES;
  const readable =
    brandColors === null ||
    ((contrastRatio(
      roleColor(brandColors, selectedAppearance.background),
      roleColor(brandColors, selectedAppearance.text),
    ) ?? 0) >= 4.5 &&
      (contrastRatio(
        roleColor(brandColors, selectedAppearance.background),
        roleColor(brandColors, selectedAppearance.hover),
      ) ?? 0) >= 4.5);

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <div
        className="inline-flex max-w-full gap-2 overflow-x-auto"
        role="tablist"
        aria-label={strings.sitesNavEditorTabs}
      >
        <button
          type="button"
          role="tab"
          aria-selected={!settingsOpen}
          className={moduleNavigationItemClassName(!settingsOpen)}
          onClick={() => setSettingsOpen(false)}
        >
          <Link2 className="size-4" aria-hidden="true" />
          {strings.sitesNavLinksTab}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={settingsOpen}
          className={moduleNavigationItemClassName(settingsOpen)}
          onClick={() => setSettingsOpen(true)}
        >
          <Settings2 className="size-4" aria-hidden="true" />
          {strings.sitesNavSettingsTab}
        </button>
      </div>

      <header className="min-w-0 px-1">
        <h3 className="m-0 text-base font-semibold text-primary">
          {settingsOpen ? strings.sitesNavSettings : strings.sitesNavMenuLinks}
        </h3>
        <p className="mt-1 text-sm leading-5 text-secondary">
          {settingsOpen
            ? strings.sitesNavSettingsHint
            : strings.sitesNavMenuLinksHint}
        </p>
      </header>

      {!settingsOpen && (
        <fieldset className="m-0 grid min-w-0 gap-3 overflow-visible border-0 p-0">
          <legend className="sr-only">{strings.sitesNavMenuLinks}</legend>
          {pagesFailed && (
            <p
              className="m-0 rounded-xl bg-warning-tint px-4 py-3 text-sm text-secondary"
              role="status"
            >
              {strings.sitesNavPagesLoadFailed}
            </p>
          )}

          <div className="flex flex-col gap-2">
            {draft.links.map((link, index) => {
              const selectedPage = pages.find(
                (page) => pagePath(page) === link.href,
              );
              const selectedSection = destinations.find(
                (target) => target.href === link.href,
              );
              const customDestination =
                selectedPage === undefined && selectedSection === undefined;
              const expanded = openLink === index;
              return (
                <div
                  key={index}
                  data-navigation-link=""
                  className="overflow-visible rounded-xl border border-subtle bg-surface"
                >
                  <div className="flex min-h-14 items-center gap-1 p-1.5">
                    <button
                      type="button"
                      className="flex min-h-11 min-w-0 flex-1 items-center gap-3 rounded-lg !px-3 !py-2 text-left text-primary transition-colors hover:!bg-raised"
                      aria-expanded={expanded}
                      onClick={() => setOpenLink(expanded ? null : index)}
                    >
                      <span className="min-w-0 flex-1">
                        <strong className="block truncate text-sm font-semibold">
                          {link.label.trim() || strings.sitesItemN(index + 1)}
                        </strong>
                        <small className="mt-0.5 block truncate text-xs text-secondary">
                          {link.href || strings.sitesNavCustomTarget}
                        </small>
                      </span>
                      <ChevronRight
                        className={`size-4 shrink-0 transition-transform ${expanded ? "rotate-90" : ""}`}
                        aria-hidden="true"
                      />
                    </button>
                    <Menu
                      label={strings.sitesColActions}
                      icon={<MoreHorizontal aria-hidden="true" />}
                      items={[
                        {
                          key: "up",
                          label: strings.sitesNavMoveLinkUp(index + 1),
                          icon: <ChevronUp aria-hidden="true" />,
                          disabled: index === 0,
                          onClick: () => reorder(index, index - 1),
                        },
                        {
                          key: "down",
                          label: strings.sitesNavMoveLinkDown(index + 1),
                          icon: <ChevronDown aria-hidden="true" />,
                          disabled: index === draft.links.length - 1,
                          onClick: () => reorder(index, index + 1),
                        },
                        {
                          key: "delete",
                          label: strings.sitesRemoveItem,
                          icon: <Trash2 aria-hidden="true" />,
                          danger: true,
                          divider: true,
                          onClick: () => {
                            setOpenLink(null);
                            onChange({
                              ...draft,
                              links: draft.links.filter(
                                (_, item) => item !== index,
                              ),
                            });
                          },
                        },
                      ]}
                    />
                  </div>

                  {expanded && (
                    <div className="grid gap-4 border-t border-subtle p-4">
                      {pages.length > 0 && (
                        <Field label={strings.sitesNavDestination}>
                          <ChoicePicker
                            value={customDestination ? "custom" : link.href}
                            label={strings.sitesNavDestination}
                            placeholder={strings.sitesNavDestination}
                            options={destinationOptions}
                            onChange={(value) => {
                              if (value === "custom") {
                                onChange({
                                  ...draft,
                                  links: draft.links.map((item, itemIndex) =>
                                    itemIndex === index
                                      ? { ...item, href: "" }
                                      : item,
                                  ),
                                });
                                return;
                              }
                              const page = pages.find(
                                (candidate) => pagePath(candidate) === value,
                              );
                              const section = destinations.find(
                                (candidate) => candidate.href === value,
                              );
                              if (page === undefined && section === undefined)
                                return;
                              onChange({
                                ...draft,
                                links: draft.links.map((item, itemIndex) =>
                                  itemIndex === index
                                    ? {
                                        label:
                                          item.label.trim() === ""
                                            ? (page?.title ??
                                              section?.defaultLabel ??
                                              "")
                                            : item.label,
                                        href: value,
                                      }
                                    : item,
                                ),
                              });
                            }}
                          />
                        </Field>
                      )}
                      <div
                        className={`grid gap-4 ${customDestination ? "sm:grid-cols-2" : ""}`}
                      >
                        <Field label={strings.sitesFieldLinkLabel}>
                          <Input
                            value={link.label}
                            onChange={(event) =>
                              onChange({
                                ...draft,
                                links: draft.links.map((item, itemIndex) =>
                                  itemIndex === index
                                    ? { ...item, label: event.target.value }
                                    : item,
                                ),
                              })
                            }
                          />
                        </Field>
                        {customDestination && (
                          <Field
                            label={strings.sitesFieldLinkHref}
                            hint={strings.sitesNavDestinationHint}
                          >
                            <Input
                              className="font-mono"
                              value={link.href}
                              autoCapitalize="none"
                              autoCorrect="off"
                              spellCheck={false}
                              onChange={(event) =>
                                onChange({
                                  ...draft,
                                  links: draft.links.map((item, itemIndex) =>
                                    itemIndex === index
                                      ? { ...item, href: event.target.value }
                                      : item,
                                  ),
                                })
                              }
                            />
                          </Field>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          <div className="flex flex-wrap items-center gap-2 pt-1">
            <Button
              variant="secondary"
              size="sm"
              icon={<Plus aria-hidden="true" />}
              onClick={() => {
                const nextIndex = draft.links.length;
                onChange({
                  ...draft,
                  links: [...draft.links, blankLink()],
                });
                setOpenLink(nextIndex);
              }}
            >
              {strings.sitesAddLink}
            </Button>
            {(pagesLoading || missingPages.length > 0) && (
              <Button
                variant="ghost"
                size="sm"
                disabled={pagesLoading}
                onClick={addSitePages}
              >
                {pagesLoading
                  ? strings.sitesNavPagesLoading
                  : strings.sitesNavAddPages}
              </Button>
            )}
          </div>
        </fieldset>
      )}

      {settingsOpen && (
        <>
          <section className="overflow-hidden rounded-xl border border-subtle bg-surface">
            <button
              type="button"
              className="flex min-h-14 w-full items-center gap-3 !px-4 !py-3 text-left text-primary transition-colors hover:!bg-raised"
              aria-expanded={actionOpen}
              onClick={() => setActionOpen((open) => !open)}
            >
              <span className="min-w-0 flex-1">
                <strong className="block text-sm font-semibold">
                  {strings.sitesNavPrimaryAction}
                </strong>
                <small className="mt-0.5 block truncate text-xs text-secondary">
                  {draft.cta.label.trim() || strings.sitesNavPrimaryActionHint}
                </small>
              </span>
              <ChevronRight
                className={`size-4 shrink-0 transition-transform ${actionOpen ? "rotate-90" : ""}`}
                aria-hidden="true"
              />
            </button>
            {actionOpen && (
              <div className="grid gap-4 border-t border-subtle p-4 sm:grid-cols-2">
                <Field label={strings.sitesFieldLinkLabel}>
                  <Input
                    value={draft.cta.label}
                    onChange={(event) =>
                      onChange({
                        ...draft,
                        cta: { ...draft.cta, label: event.target.value },
                      })
                    }
                  />
                </Field>
                <Field label={strings.sitesFieldLinkHref}>
                  <Input
                    className="font-mono"
                    value={draft.cta.href}
                    autoCapitalize="none"
                    autoCorrect="off"
                    spellCheck={false}
                    onChange={(event) =>
                      onChange({
                        ...draft,
                        cta: { ...draft.cta, href: event.target.value },
                      })
                    }
                  />
                </Field>
              </div>
            )}
          </section>

          <section className="overflow-hidden rounded-xl border border-subtle bg-surface">
            <button
              type="button"
              className="flex min-h-14 w-full items-center gap-3 !px-4 !py-3 text-left text-primary transition-colors hover:!bg-raised"
              aria-label={strings.sitesNavAppearanceShow}
              aria-expanded={appearanceOpen}
              onClick={() => setAppearanceOpen((open) => !open)}
            >
              <span className="min-w-0 flex-1">
                <strong className="block text-sm font-semibold">
                  {strings.sitesNavAppearance}
                </strong>
                <small className="mt-0.5 block text-xs text-secondary">
                  {draft.appearance === undefined
                    ? strings.sitesNavUsesTheme
                    : strings.sitesNavUsesBrandRoles}
                </small>
              </span>
              <ChevronRight
                className={`size-4 shrink-0 transition-transform ${appearanceOpen ? "rotate-90" : ""}`}
                aria-hidden="true"
              />
            </button>
            {appearanceOpen && (
              <div className="border-t border-subtle p-4">
                <div className="grid gap-4 sm:grid-cols-3">
                  {(
                    [
                      ["background", strings.sitesNavBackground],
                      ["text", strings.sitesNavText],
                      ["hover", strings.sitesNavHover],
                    ] as const
                  ).map(([property, label]) => {
                    const selected =
                      draft.appearance?.[property] ??
                      NAV_DEFAULT_ROLES[property];
                    return (
                      <Field key={property} label={label}>
                        <ChoicePicker
                          value={selected}
                          label={label}
                          placeholder={label}
                          options={appearanceOptions}
                          onChange={(value) =>
                            onChange({
                              ...draft,
                              appearance: {
                                ...(draft.appearance ?? NAV_DEFAULT_ROLES),
                                [property]: value as ThemeColorRole,
                              },
                            })
                          }
                        />
                      </Field>
                    );
                  })}
                </div>
                <div className="mt-4 flex items-center justify-between gap-3">
                  {!readable && (
                    <p className="m-0 text-xs text-danger">
                      {strings.sitesNavContrastFail}
                    </p>
                  )}
                  {draft.appearance !== undefined && (
                    <Button
                      className="ml-auto"
                      variant="ghost"
                      size="sm"
                      onClick={() =>
                        onChange({ ...draft, appearance: undefined })
                      }
                    >
                      {strings.sitesNavResetRoles}
                    </Button>
                  )}
                </div>
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}

function HeroFields({
  draft,
  onChange,
}: {
  draft: HeroDraft;
  onChange: Change;
}) {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const workspaceBrand = readBrandKit();
  const [brandColors, setBrandColors] = useState<BrandColors | null>(null);
  useEffect(() => {
    if (siteId === "") return;
    let current = true;
    void Promise.all([api.site(siteId), api.themePresets()])
      .then(([site, presets]) => {
        if (!current) return;
        const preset = presets.find(
          (item) => item.id === (site.theme.preset ?? presets[0]?.id),
        );
        if (preset !== undefined)
          setBrandColors(themeColors(preset, site.theme.colors));
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [api, siteId]);

  const visibleColors = brandColors ?? HERO_FALLBACK_COLORS;
  const colorOptions = [
    { value: "background", label: strings.sitesThemeBackgroundColor, swatch: visibleColors.background },
    { value: "text", label: strings.sitesThemeTextColor, swatch: visibleColors.text },
    { value: "border", label: strings.sitesThemeBorderColor, swatch: visibleColors.border },
    { value: "accent_1", label: strings.sitesThemeAccentColor(1), swatch: visibleColors.accent_1 },
    ...(workspaceBrand.secondary === null
      ? []
      : [{ value: "accent_2", label: strings.sitesThemeAccentColor(2), swatch: visibleColors.accent_2 }]),
    ...workspaceBrand.supporting.map((color, index) => ({
      value: `accent_${index + 3}`,
      label: color.name,
      swatch: visibleColors[`accent_${index + 3}` as keyof BrandColors],
    })),
  ];
  const selectedColors = draft.appearance ?? HERO_DEFAULT_ROLES;
  const explicitTextRole = (
    property:
      | "primary_button_text"
      | "primary_button_hover_text"
      | "secondary_button_text"
      | "secondary_button_hover_text",
  ) => draft.appearance?.[property] ?? "auto";
  const readable =
    brandColors === null ||
    ([
      [selectedColors.primary_button, draft.appearance?.primary_button_text],
      [
        selectedColors.primary_button_hover,
        draft.appearance?.primary_button_hover_text,
      ],
      [selectedColors.secondary_button, draft.appearance?.secondary_button_text],
      [
        selectedColors.secondary_button_hover,
        draft.appearance?.secondary_button_hover_text,
      ],
    ] as const).every(
      ([background, text]) =>
        text === undefined ||
        (contrastRatio(
          roleColor(brandColors, background),
          roleColor(brandColors, text),
        ) ?? 0) >= 4.5,
    );
  const layouts = [
    { value: "centered", label: strings.sitesHeroLayoutCentered },
    { value: "split_right", label: strings.sitesHeroLayoutSplitRight },
    { value: "split_left", label: strings.sitesHeroLayoutSplitLeft },
    { value: "background", label: strings.sitesHeroLayoutBackground },
    {
      value: "video_background",
      label: strings.sitesHeroLayoutVideoBackground,
    },
    { value: "editorial", label: strings.sitesHeroLayoutEditorial },
  ] as const;

  return (
    <div className="grid gap-6">
      <fieldset>
        <legend className="text-base font-semibold text-primary">
          {strings.sitesHeroLayout}
        </legend>
        <p className="mb-4 mt-1 text-sm text-secondary">
          {strings.sitesHeroLayoutHint}
        </p>
        <div
          className="grid grid-cols-2 gap-3 md:grid-cols-3"
          role="radiogroup"
          aria-label={strings.sitesHeroLayout}
        >
          {layouts.map((layout) => {
            const selected = draft.layout === layout.value;
            return (
              <button
                key={layout.value}
                type="button"
                role="radio"
                aria-checked={selected}
                className={cx(
                  "group relative min-w-0 rounded-2xl !border-2 !p-5 text-left transition-[background-color,border-color,box-shadow] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30",
                  selected
                    ? "!border-accent bg-accent-soft/50 shadow-sm"
                    : "!border-default bg-surface hover:!border-accent/40 hover:bg-accent-soft/20",
                )}
                onClick={() => onChange({ ...draft, layout: layout.value })}
              >
                <HeroLayoutVisual layout={layout.value} />
                <span className="mt-3 block min-h-10 text-sm font-semibold leading-5 text-primary">
                  {layout.label}
                </span>
                {selected && (
                  <span className="absolute right-5 top-5 grid size-5 place-items-center rounded-full bg-accent text-on-accent shadow-sm ring-2 ring-surface">
                    <Check className="size-3" aria-hidden="true" />
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </fieldset>

      <Card as="section" flat>
        <h3 className="m-0 text-base font-semibold text-primary">
          {strings.sitesHeroDesign}
        </h3>
        <div className="mt-4 grid gap-4 lg:grid-cols-3">
          <HeroOptionRow
            label={strings.sitesHeroHeight}
            value={draft.height}
            visual={(option) => (
              <HeroControlVisual group="height" value={option} />
            )}
            options={[
              ["compact", strings.sitesHeroHeightCompact],
              ["standard", strings.sitesHeroHeightStandard],
              ["tall", strings.sitesHeroHeightTall],
            ]}
            onChange={(height) => onChange({ ...draft, height })}
          />
          <HeroOptionRow
            label={strings.sitesHeroAlignment}
            value={draft.alignment}
            visual={(option) => (
              <HeroControlVisual group="alignment" value={option} />
            )}
            options={[
              ["left", strings.sitesHeroAlignmentLeft],
              ["center", strings.sitesHeroAlignmentCenter],
              ["right", strings.sitesHeroAlignmentRight],
            ]}
            onChange={(alignment) => onChange({ ...draft, alignment })}
          />
          <HeroOptionRow
            label={strings.sitesHeroContentWidth}
            value={draft.content_width}
            visual={(option) => (
              <HeroControlVisual group="width" value={option} />
            )}
            options={[
              ["narrow", strings.sitesHeroContentWidthNarrow],
              ["balanced", strings.sitesHeroContentWidthBalanced],
              ["wide", strings.sitesHeroContentWidthWide],
            ]}
            onChange={(content_width) => onChange({ ...draft, content_width })}
          />
        </div>
      </Card>

      <Card as="section" flat>
        <HeroFormHeading icon={<Palette size={17} />}>
          {strings.sitesHeroColors}
        </HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">
          {strings.sitesHeroColorsHint}
        </p>
        <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.35fr)]">
          <HeroColorSwatches
            label={strings.sitesHeroBackgroundColor}
            value={selectedColors.background}
            options={colorOptions}
            onChange={(value) =>
              onChange({
                ...draft,
                appearance: {
                  ...(draft.appearance ?? HERO_DEFAULT_ROLES),
                  background: value as ThemeColorRole,
                },
              })
            }
          />
          <div className="grid gap-2">
            <span className="text-sm font-semibold text-primary">
              {strings.sitesHeroButtonLayout}
            </span>
            <div
              className="grid grid-cols-3 gap-2"
              role="radiogroup"
              aria-label={strings.sitesHeroButtonLayout}
            >
              {(
                [
                  [0, strings.sitesHeroNoButtons],
                  [1, strings.sitesHeroOneButton],
                  [2, strings.sitesHeroTwoButtons],
                ] as const
              ).map(([count, label]) => (
                <button
                  key={count}
                  type="button"
                  role="radio"
                  aria-checked={draft.button_count === count}
                  aria-label={label}
                  className={cx(
                    "grid min-h-16 place-items-center gap-1.5 rounded-xl !border !px-3 !py-2 text-xs font-semibold transition-[border-color,background-color,box-shadow] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30",
                    draft.button_count === count
                      ? "!border-accent bg-accent-soft/50 text-accent shadow-sm"
                      : "!border-default bg-surface text-secondary hover:!border-accent/40",
                  )}
                  onClick={() => onChange({ ...draft, button_count: count })}
                >
                  <span className="flex h-4 items-center justify-center gap-1" aria-hidden="true">
                    {Array.from({ length: count }, (_, index) => (
                      <span
                        key={index}
                        className={cx(
                          "h-2.5 rounded-full",
                          index === 0 ? "w-8 bg-accent" : "w-6 border border-accent",
                        )}
                      />
                    ))}
                    {count === 0 && <span className="h-px w-8 bg-border" />}
                  </span>
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>
        {draft.button_count > 0 && (
          <div className="mt-5 grid gap-4 lg:grid-cols-2">
          {(
            [
              [
                strings.sitesHeroPrimaryButtonColor,
                "primary_button",
                "primary_button_text",
                "primary_button_hover",
                "primary_button_hover_text",
                true,
              ],
              [
                strings.sitesHeroSecondaryButtonColor,
                "secondary_button",
                "secondary_button_text",
                "secondary_button_hover",
                "secondary_button_hover_text",
                draft.button_count === 2,
              ],
            ] as const
          ).filter(([, , , , , visible]) => visible)
          .map(([title, color, text, hover, hoverText]) => (
            <section key={title} className="rounded-xl border border-subtle bg-surface p-4">
              <h4 className="m-0 text-sm font-semibold text-primary">{title}</h4>
              <div className="mt-4 grid gap-4 sm:grid-cols-2">
                <HeroColorSwatches
                    label={`${title}: ${strings.sitesNavBackground}`}
                    value={selectedColors[color]}
                    options={colorOptions}
                    onChange={(value) =>
                      onChange({
                        ...draft,
                        appearance: {
                          ...(draft.appearance ?? HERO_DEFAULT_ROLES),
                          [color]: value as ThemeColorRole,
                        },
                      })
                    }
                  />
                <HeroColorSwatches
                    label={`${title}: ${strings.sitesNavText}`}
                    value={explicitTextRole(text)}
                    options={colorOptions}
                    automatic
                    onChange={(value) =>
                      onChange({
                        ...draft,
                        appearance: {
                          ...(draft.appearance ?? HERO_DEFAULT_ROLES),
                          [text]: value === "auto" ? undefined : (value as ThemeColorRole),
                        },
                      })
                    }
                  />
                <HeroColorSwatches
                    label={`${title}: ${strings.sitesHeroHoverBackground}`}
                    value={selectedColors[hover]}
                    options={colorOptions}
                    onChange={(value) =>
                      onChange({
                        ...draft,
                        appearance: {
                          ...(draft.appearance ?? HERO_DEFAULT_ROLES),
                          [hover]: value as ThemeColorRole,
                        },
                      })
                    }
                  />
                <HeroColorSwatches
                    label={`${title}: ${strings.sitesHeroHoverText}`}
                    value={explicitTextRole(hoverText)}
                    options={colorOptions}
                    automatic
                    onChange={(value) =>
                      onChange({
                        ...draft,
                        appearance: {
                          ...(draft.appearance ?? HERO_DEFAULT_ROLES),
                          [hoverText]:
                            value === "auto" ? undefined : (value as ThemeColorRole),
                        },
                      })
                    }
                  />
              </div>
            </section>
          ))}
          </div>
        )}
        {draft.button_count > 0 && !readable && (
          <p className="mb-0 mt-4 text-sm text-danger" role="status">
            {strings.sitesHeroButtonContrastWarning}
          </p>
        )}
        {draft.appearance !== undefined && (
          <div className="mt-4 flex justify-end">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => onChange({ ...draft, appearance: undefined })}
            >
              {strings.sitesHeroUseThemeColors}
            </Button>
          </div>
        )}
      </Card>

      <Card as="section" flat>
        <h3 className="m-0 text-base font-semibold text-primary">
          {strings.sitesHeroAnimation}
        </h3>
        <p className="mb-5 mt-1 text-sm text-secondary">
          {strings.sitesHeroAnimationHint}
        </p>
        <div className="grid gap-5">
          <HeroOptionRow
            label={strings.sitesHeroTextAnimation}
            value={draft.text_animation}
            columns={4}
            visual={(option) => (
              <HeroControlVisual group="text" value={option} />
            )}
            options={[
              ["none", strings.sitesHeroAnimationNone],
              ["fade_up", strings.sitesHeroTextFadeUp],
              ["word_reveal", strings.sitesHeroTextWordReveal],
              ["slide_in", strings.sitesHeroTextSlideIn],
            ]}
            onChange={(text_animation) =>
              onChange({ ...draft, text_animation })
            }
          />
          <HeroOptionRow
            label={strings.sitesHeroMediaAnimation}
            value={draft.media_animation}
            columns={4}
            visual={(option) => (
              <HeroControlVisual group="media" value={option} />
            )}
            options={[
              ["none", strings.sitesHeroAnimationNone],
              ["fade_in", strings.sitesHeroMediaFadeIn],
              ["slide_up", strings.sitesHeroMediaSlideUp],
              ["slow_zoom", strings.sitesHeroMediaSlowZoom],
            ]}
            onChange={(media_animation) =>
              onChange({ ...draft, media_animation })
            }
          />
          <div className="max-w-xl">
            <HeroOptionRow
              label={strings.sitesHeroAnimationSpeed}
              value={draft.animation_speed}
              visual={(option) => (
                <HeroControlVisual group="pace" value={option} />
              )}
              options={[
                ["quick", strings.sitesHeroAnimationQuick],
                ["smooth", strings.sitesHeroAnimationSmooth],
                ["relaxed", strings.sitesHeroAnimationRelaxed],
              ]}
              onChange={(animation_speed) =>
                onChange({ ...draft, animation_speed })
              }
            />
          </div>
        </div>
      </Card>

      <Card as="section" flat>
        <div className="grid items-start gap-8 lg:grid-cols-2">
          <div className="grid gap-4">
            <HeroFormHeading icon={<Type size={17} />}>
              {strings.sitesHeroContent}
            </HeroFormHeading>
            <TextField
              label={strings.sitesFieldHeading}
              value={draft.heading}
              onChange={(heading) => onChange({ ...draft, heading })}
              autoFocus
              copyPointer="/heading"
            />
            <TextField
              label={strings.sitesFieldSubheading}
              value={draft.subheading}
              onChange={(subheading) => onChange({ ...draft, subheading })}
              copyPointer="/subheading"
            />
          </div>
          <div className="grid gap-4">
            <HeroFormHeading icon={<ImageIcon size={17} />}>
              {strings.sitesHeroMedia}
            </HeroFormHeading>
            {draft.layout === "video_background" && (
              <div className="grid gap-2">
                <TextField
                  label={strings.sitesHeroVideoUrl}
                  value={draft.video_url}
                  onChange={(video_url) => onChange({ ...draft, video_url })}
                  hint={strings.sitesHeroVideoUrlHint}
                  mono
                />
                <p className="m-0 text-sm text-secondary">
                  {strings.sitesHeroVideoFallbackHint}
                </p>
              </div>
            )}
            <ImageFields
              bare
              value={draft.image}
              pointer="/image"
              onChange={(patch) =>
                onChange({ ...draft, image: { ...draft.image, ...patch } })
              }
            />
          </div>
        </div>
      </Card>

      {draft.button_count > 0 && (
        <Card as="section" flat>
          <HeroFormHeading icon={<MousePointerClick size={17} />}>
            {strings.sitesHeroActions}
          </HeroFormHeading>
          <div className="mt-5 grid gap-6 lg:grid-cols-2">
            <LinkFields
              bare
              legend={strings.sitesFieldPrimaryButton}
              value={draft.primary_cta}
              onChange={(patch) =>
                onChange({
                  ...draft,
                  primary_cta: { ...draft.primary_cta, ...patch },
                })
              }
            />
            {draft.button_count === 2 && (
              <LinkFields
                bare
                legend={strings.sitesFieldSecondaryButton}
                value={draft.secondary_cta}
                onChange={(patch) =>
                  onChange({
                    ...draft,
                    secondary_cta: { ...draft.secondary_cta, ...patch },
                  })
                }
              />
            )}
          </div>
        </Card>
      )}
    </div>
  );
}

function HeroFormHeading({
  icon,
  children,
}: {
  icon: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center gap-3">
      <span
        className="grid size-9 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent"
        aria-hidden="true"
      >
        {icon}
      </span>
      <h3 className="m-0 text-base font-semibold text-primary">{children}</h3>
    </div>
  );
}

function HeroLayoutVisual({ layout }: { layout: HeroDraft["layout"] }) {
  const copy = (
    <span className="grid content-center gap-2">
      <span className="h-2 w-4/5 rounded-full bg-primary/75" />
      <span className="h-1.5 w-full rounded-full bg-secondary/25" />
      <span className="h-1.5 w-2/3 rounded-full bg-secondary/25" />
      <span className="mt-1 h-4 w-14 rounded-md bg-accent" />
    </span>
  );
  const image = <span className="min-h-16 rounded-lg bg-accent-soft" />;

  return (
    <span
      className={cx(
        "block h-24 overflow-hidden rounded-xl bg-raised p-3",
        (layout === "background" || layout === "video_background") &&
          "grid content-center bg-accent-soft px-5 text-center",
        layout === "editorial" && "border-l-4 border-accent",
      )}
      aria-hidden="true"
    >
      {layout === "centered" && (
        <span className="mx-auto grid max-w-24 justify-items-center gap-2 pt-2">
          <span className="h-2 w-4/5 rounded-full bg-primary/75" />
          <span className="h-1.5 w-full rounded-full bg-secondary/25" />
          <span className="h-1.5 w-2/3 rounded-full bg-secondary/25" />
          <span className="mt-1 h-4 w-14 rounded-md bg-accent" />
        </span>
      )}
      {layout === "split_right" && (
        <span className="grid h-full grid-cols-2 gap-2">
          {copy}
          {image}
        </span>
      )}
      {layout === "split_left" && (
        <span className="grid h-full grid-cols-2 gap-2">
          {image}
          {copy}
        </span>
      )}
      {layout === "background" && (
        <span className="grid justify-items-center gap-2">
          <span className="h-2 w-4/5 rounded-full bg-primary/75" />
          <span className="h-1.5 w-full rounded-full bg-primary/25" />
          <span className="h-4 w-14 rounded-md bg-accent" />
        </span>
      )}
      {layout === "video_background" && (
        <span className="relative grid justify-items-center gap-2">
          <span className="h-2 w-4/5 rounded-full bg-primary/75" />
          <span className="h-1.5 w-full rounded-full bg-primary/25" />
          <span className="h-4 w-14 rounded-md bg-accent" />
          <span className="absolute right-1 top-1 grid size-8 place-items-center rounded-full bg-surface/90 text-accent shadow-sm">
            <Play className="size-4 fill-current" />
          </span>
        </span>
      )}
      {layout === "editorial" && (
        <span className="grid h-full content-center gap-2 pl-1">
          <span className="h-2.5 w-4/5 rounded-full bg-primary/75" />
          <span className="h-2.5 w-3/5 rounded-full bg-primary/75" />
          <span className="h-1.5 w-full rounded-full bg-secondary/25" />
          <span className="mt-1 h-4 w-14 rounded-md bg-accent" />
        </span>
      )}
    </span>
  );
}

function HeroOptionRow<T extends string>({
  label,
  value,
  options,
  columns = 3,
  visual,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly (readonly [T, string])[];
  columns?: 3 | 4 | 5;
  visual?: (value: T) => ReactNode;
  onChange: (value: T) => void;
}) {
  const illustrated = visual !== undefined;
  return (
    <fieldset>
      <legend className="mb-2 text-sm font-semibold text-primary">
        {label}
      </legend>
      <div
        className={cx(
          "grid",
          illustrated ? "gap-2" : "gap-1 rounded-xl bg-raised p-1",
          columns === 5
            ? "grid-cols-2 sm:grid-cols-3 lg:grid-cols-5"
            : columns === 4
              ? "grid-cols-2 sm:grid-cols-4"
              : "grid-cols-3",
        )}
        role="radiogroup"
        aria-label={label}
      >
        {options.map(([option, optionLabel]) => {
          const selected = option === value;
          return (
            <button
              key={option}
              type="button"
              role="radio"
              aria-checked={selected}
              className={cx(
                "min-w-0 text-sm font-medium transition-[background-color,border-color,color,box-shadow] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30",
                illustrated
                  ? "relative flex min-h-28 flex-col items-center justify-between rounded-xl !border !border-default !p-4"
                  : "min-h-11 rounded-lg !px-2",
                selected && illustrated
                  ? "!border-accent bg-accent-soft/50 text-primary shadow-sm"
                  : selected
                    ? "bg-surface text-accent shadow-sm"
                    : illustrated
                      ? "bg-surface text-secondary hover:!border-accent/40 hover:bg-accent-soft/20"
                      : "text-secondary hover:bg-surface/60 hover:text-primary",
              )}
              onClick={() => onChange(option)}
            >
              {visual?.(option)}
              <span
                className={cx(
                  "block whitespace-normal leading-tight",
                  illustrated && "mt-3 min-h-8 text-center",
                )}
              >
                {optionLabel}
              </span>
              {selected && illustrated && (
                <span className="absolute right-5 top-5 grid size-5 place-items-center rounded-full bg-accent text-on-accent shadow-sm ring-2 ring-surface">
                  <Check className="size-3" aria-hidden="true" />
                </span>
              )}
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}

function HeroControlVisual({
  group,
  value,
}: {
  group: "height" | "alignment" | "width" | "text" | "media" | "pace";
  value: string;
}) {
  if (group === "height") {
    const height =
      value === "compact" ? "h-3" : value === "tall" ? "h-9" : "h-6";
    return (
      <span
        className="flex h-12 items-end"
        data-hero-control-visual={`${group}:${value}`}
        aria-hidden="true"
      >
        <span className="flex h-11 w-16 items-end rounded-lg border border-default bg-raised p-1.5">
          <span
            className={cx(
              "block w-full rounded-md border border-accent/40 bg-accent-soft",
              height,
            )}
          />
        </span>
      </span>
    );
  }

  if (group === "alignment") {
    const alignment =
      value === "left"
        ? "items-start"
        : value === "right"
          ? "items-end"
          : "items-center";
    return (
      <span
        className={cx(
          "flex h-12 w-16 flex-col justify-center gap-1.5",
          alignment,
        )}
        data-hero-control-visual={`${group}:${value}`}
        aria-hidden="true"
      >
        <span className="h-1.5 w-14 rounded-full bg-primary/70" />
        <span className="h-1.5 w-11 rounded-full bg-secondary/30" />
        <span className="h-1.5 w-8 rounded-full bg-accent" />
      </span>
    );
  }

  if (group === "width") {
    const width =
      value === "narrow" ? "w-8" : value === "wide" ? "w-16" : "w-12";
    return (
      <span
        className="flex h-12 w-16 flex-col items-center justify-center gap-1.5"
        data-hero-control-visual={`${group}:${value}`}
        aria-hidden="true"
      >
        <span className={cx("h-1.5 rounded-full bg-primary/70", width)} />
        <span className={cx("h-1.5 rounded-full bg-secondary/30", width)} />
        <span className={cx("h-1.5 rounded-full bg-accent/70", width)} />
      </span>
    );
  }

  if (group === "text") {
    return (
      <span
        className="flex h-12 w-20 items-center justify-center gap-1.5 overflow-hidden"
        data-hero-control-visual={`${group}:${value}`}
        aria-hidden="true"
      >
        {[0, 1, 2].map((index) => (
          <span
            key={index}
            className={cx(
              "h-5 rounded-md bg-primary/65",
              index === 0 ? "w-5" : index === 1 ? "w-6" : "w-4",
              value === "fade_up" && index === 0 && "translate-y-2 opacity-25",
              value === "fade_up" && index === 1 && "translate-y-1 opacity-55",
              value === "word_reveal" && index === 0 && "opacity-25",
              value === "word_reveal" && index === 1 && "opacity-55",
              value === "word_reveal" && index === 2 && "bg-accent",
              value === "slide_in" &&
                index === 0 &&
                "-translate-x-4 opacity-25",
              value === "slide_in" &&
                index === 1 &&
                "-translate-x-2 opacity-55",
            )}
          />
        ))}
      </span>
    );
  }

  if (group === "media") {
    return (
      <span
        className="relative flex h-12 w-20 items-center justify-center overflow-hidden rounded-lg border border-default bg-raised"
        data-hero-control-visual={`${group}:${value}`}
        aria-hidden="true"
      >
        {value === "slide_up" && (
          <span className="absolute top-1 size-8 rounded-md border border-accent/30 bg-accent-soft/30" />
        )}
        <span
          className={cx(
            "size-9 rounded-md border border-accent/40 bg-accent-soft",
            value === "fade_in" && "opacity-45",
            value === "slide_up" && "translate-y-2",
            value === "slow_zoom" && "scale-110 shadow-sm",
          )}
        >
          <span className="mx-auto mt-2 block size-2 rounded-full bg-accent/70" />
          <span className="mx-auto mt-1 block h-1.5 w-6 rounded-full bg-primary/25" />
        </span>
      </span>
    );
  }

  const gap =
    value === "quick" ? "gap-1" : value === "relaxed" ? "gap-4" : "gap-2.5";
  return (
    <span
      className={cx("flex h-12 items-center justify-center", gap)}
      data-hero-control-visual={`${group}:${value}`}
      aria-hidden="true"
    >
      <span className="size-2.5 rounded-full bg-accent/25" />
      <span className="size-2.5 rounded-full bg-accent/55" />
      <span className="size-2.5 rounded-full bg-accent" />
    </span>
  );
}

function FeaturesLayoutVisual({ layout }: { layout: FeaturesDraft["layout"] }) {
  const cards = layout === "list" || layout === "spotlight" ? 3 : 4;
  return (
    <span className={cx("grid h-20 w-32 gap-1.5 rounded-lg bg-raised p-2", layout === "list" ? "grid-cols-1" : "grid-cols-2")} aria-hidden="true">
      {Array.from({ length: cards }, (_, index) => (
        <span key={index} className={cx(
          "relative rounded-md border border-accent/25 bg-accent-soft",
          layout === "bento" && index === 1 && "row-span-2",
          layout === "spotlight" && index === 0 && "col-span-2 h-8",
          layout === "steps" && "before:absolute before:left-1 before:top-1 before:size-2 before:rounded-full before:bg-accent",
        )} />
      ))}
    </span>
  );
}

function FeaturesFields({
  draft,
  onChange,
}: {
  draft: FeaturesDraft;
  onChange: Change;
}) {
  return (
    <>
      <Card as="section" flat>
        <HeroFormHeading icon={<PanelsTopLeft size={17} />}>{strings.sitesFeaturesLayout}</HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">{strings.sitesFeaturesLayoutHint}</p>
        <HeroOptionRow
          label={strings.sitesFeaturesLayout}
          value={draft.layout}
          columns={5}
          visual={(layout) => <FeaturesLayoutVisual layout={layout} />}
          options={[
            ["grid", strings.sitesFeaturesLayoutGrid],
            ["bento", strings.sitesFeaturesLayoutBento],
            ["list", strings.sitesFeaturesLayoutList],
            ["steps", strings.sitesFeaturesLayoutSteps],
            ["spotlight", strings.sitesFeaturesLayoutSpotlight],
          ]}
          onChange={(layout) => onChange({ ...draft, layout })}
        />
      </Card>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesFieldIntro}
        value={draft.intro}
        onChange={(intro) => onChange({ ...draft, intro })}
        copyPointer="/intro"
      />
      <ItemsEditor
        addLabel={strings.sitesAddEntry}
        items={draft.items}
        onChange={(items) => onChange({ ...draft, items })}
        blank={blankFeature}
        render={(item, update, index) => (
          <>
            <TextField
              label={strings.sitesFieldItemTitle}
              value={item.title}
              onChange={(title) => update({ title })}
              copyPointer={`/items/${index}/title`}
            />
            <LongTextField
              label={strings.sitesFieldBody}
              value={item.body}
              onChange={(body) => update({ body })}
              copyPointer={`/items/${index}/body`}
            />
          </>
        )}
      />
    </>
  );
}

function TextImageLayoutVisual({ layout }: { layout: TextImageDraft["layout"] }) {
  return (
    <span className={cx("grid h-20 w-32 grid-cols-2 items-center gap-1.5 overflow-hidden rounded-lg bg-raised p-2", layout === "full_bleed" && "p-0")} aria-hidden="true">
      <span className={cx(
        "h-14 rounded-md bg-accent-soft",
        layout === "overlap" && "z-10 translate-x-2",
        layout === "framed" && "border-4 border-surface ring-1 ring-default",
        layout === "full_bleed" && "h-full rounded-none",
      )} />
      <span className={cx("grid gap-1.5", layout === "overlap" && "z-20 -translate-x-2 rounded-md bg-surface p-2 shadow-sm")}>
        <span className={cx("h-2 rounded-full bg-primary/70", layout === "editorial" && "h-3")} />
        <span className="h-1.5 rounded-full bg-secondary/25" />
        <span className="h-1.5 w-2/3 rounded-full bg-accent/60" />
      </span>
    </span>
  );
}

function TextImageFields({
  draft,
  onChange,
}: {
  draft: TextImageDraft;
  onChange: Change;
}) {
  return (
    <>
      <Card as="section" flat>
        <HeroFormHeading icon={<PanelsTopLeft size={17} />}>{strings.sitesTextImageLayout}</HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">{strings.sitesTextImageLayoutHint}</p>
        <HeroOptionRow
          label={strings.sitesTextImageLayout}
          value={draft.layout}
          columns={5}
          visual={(layout) => <TextImageLayoutVisual layout={layout} />}
          options={[["split", strings.sitesTextImageLayoutSplit], ["overlap", strings.sitesTextImageLayoutOverlap], ["framed", strings.sitesTextImageLayoutFramed], ["editorial", strings.sitesTextImageLayoutEditorial], ["full_bleed", strings.sitesTextImageLayoutFullBleed]]}
          onChange={(layout) => onChange({ ...draft, layout })}
        />
        <div className="mt-5 max-w-md">
          <HeroOptionRow
            label={strings.sitesFieldImageSide}
            value={draft.image_side}
            visual={(side) => <HeroControlVisual group="alignment" value={side} />}
            options={[["left", strings.sitesSideLeft], ["right", strings.sitesSideRight]]}
            onChange={(image_side) => onChange({ ...draft, image_side })}
          />
        </div>
      </Card>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesFieldBody}
        value={draft.body}
        onChange={(body) => onChange({ ...draft, body })}
        copyPointer="/body"
      />
      <ImageFields
        legend={strings.sitesFieldImage}
        value={draft.image}
        pointer="/image"
        onChange={(patch) =>
          onChange({ ...draft, image: { ...draft.image, ...patch } })
        }
      />
    </>
  );
}

function GalleryFields({
  draft,
  onChange,
}: {
  draft: GalleryDraft;
  onChange: Change;
}) {
  return (
    <>
      <Card as="section" flat>
        <HeroFormHeading icon={<PanelsTopLeft size={17} />}>
          {strings.sitesGalleryLayout}
        </HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">
          {strings.sitesGalleryLayoutHint}
        </p>
        <HeroOptionRow
          label={strings.sitesGalleryLayout}
          value={draft.layout}
          columns={5}
          visual={(layout) => <GalleryLayoutVisual layout={layout} />}
          options={[
            ["grid", strings.sitesGalleryLayoutGrid],
            ["masonry", strings.sitesGalleryLayoutMasonry],
            ["collage", strings.sitesGalleryLayoutCollage],
            ["filmstrip", strings.sitesGalleryLayoutFilmstrip],
            ["spotlight", strings.sitesGalleryLayoutSpotlight],
          ]}
          onChange={(layout) => onChange({ ...draft, layout })}
        />
      </Card>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <ItemsEditor
        addLabel={strings.sitesAddImage}
        items={draft.images}
        onChange={(images) => onChange({ ...draft, images })}
        blank={blankImage}
        render={(image, update, index) => (
          <ImageFields
            value={image}
            onChange={update}
            pointer={`/images/${index}`}
          />
        )}
      />
    </>
  );
}

function GalleryLayoutVisual({ layout }: { layout: GalleryDraft["layout"] }) {
  if (layout === "filmstrip") {
    return (
      <span
        className="flex h-20 w-32 items-center gap-1.5 overflow-hidden rounded-lg bg-raised p-2"
        aria-hidden="true"
      >
        {[0, 1, 2].map((item) => (
          <span
            key={item}
            className="h-14 w-12 shrink-0 rounded-md bg-accent-soft"
          />
        ))}
      </span>
    );
  }

  return (
    <span
      className="grid h-20 w-32 grid-cols-3 grid-rows-2 gap-1.5 rounded-lg bg-raised p-2"
      aria-hidden="true"
    >
      {[0, 1, 2, 3, 4, 5].map((item) => (
        <span
          key={item}
          className={cx(
            "rounded-md bg-accent-soft",
            layout === "masonry" && item % 2 === 0 && "row-span-2",
            layout === "collage" && item === 0 && "col-span-2 row-span-2",
            layout === "collage" && (item === 3 || item === 4 || item === 5) && "hidden",
            layout === "spotlight" && item === 0 && "col-span-3",
            layout === "spotlight" && (item === 4 || item === 5) && "hidden",
          )}
        />
      ))}
    </span>
  );
}

function TestimonialsLayoutVisual({
  layout,
}: {
  layout: TestimonialsDraft["layout"];
}) {
  const cards = layout === "carousel" ? 3 : layout === "stacked" ? 2 : 4;
  return (
    <span
      className={cx(
        "grid h-20 w-32 gap-1.5 overflow-hidden rounded-lg bg-raised p-2",
        layout === "cards" && "grid-cols-2",
        layout === "featured" && "grid-cols-2",
        layout === "editorial" && "grid-cols-2 gap-x-3 bg-transparent",
        layout === "stacked" && "grid-cols-1 px-5",
        layout === "carousel" && "grid-flow-col grid-cols-[repeat(3,3rem)]",
      )}
      aria-hidden="true"
    >
      {Array.from({ length: cards }, (_, item) => (
        <span
          key={item}
          className={cx(
            "grid content-center gap-1 rounded-md border border-default bg-surface p-1.5",
            layout === "featured" && item === 0 && "col-span-2",
            layout === "editorial" && "border-0 bg-transparent p-0",
          )}
        >
          <span className="h-1.5 rounded-full bg-primary/70" />
          <span className="h-1 w-2/3 rounded-full bg-accent/60" />
        </span>
      ))}
    </span>
  );
}

function TestimonialsFields({
  draft,
  onChange,
}: {
  draft: TestimonialsDraft;
  onChange: Change;
}) {
  return (
    <>
      <Card as="section" flat>
        <HeroFormHeading icon={<PanelsTopLeft size={17} />}>
          {strings.sitesTestimonialsLayout}
        </HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">
          {strings.sitesTestimonialsLayoutHint}
        </p>
        <HeroOptionRow
          label={strings.sitesTestimonialsLayout}
          value={draft.layout}
          columns={5}
          visual={(layout) => <TestimonialsLayoutVisual layout={layout} />}
          options={[
            ["cards", strings.sitesTestimonialsLayoutCards],
            ["featured", strings.sitesTestimonialsLayoutFeatured],
            ["editorial", strings.sitesTestimonialsLayoutEditorial],
            ["stacked", strings.sitesTestimonialsLayoutStacked],
            ["carousel", strings.sitesTestimonialsLayoutCarousel],
          ]}
          onChange={(layout) => onChange({ ...draft, layout })}
        />
      </Card>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <ItemsEditor
        addLabel={strings.sitesAddEntry}
        items={draft.items}
        onChange={(items) => onChange({ ...draft, items })}
        blank={blankTestimonial}
        render={(item, update, index) => (
          <>
            <LongTextField
              label={strings.sitesFieldQuote}
              value={item.quote}
              onChange={(quote) => update({ quote })}
              copyPointer={`/items/${index}/quote`}
            />
            <TextField
              label={strings.sitesFieldAuthor}
              value={item.author}
              onChange={(author) => update({ author })}
            />
            <TextField
              label={strings.sitesFieldRole}
              value={item.role}
              onChange={(role) => update({ role })}
              copyPointer={`/items/${index}/role`}
            />
          </>
        )}
      />
    </>
  );
}

function PricingLayoutVisual({ layout }: { layout: PricingDraft["layout"] }) {
  const count = layout === "compact" ? 3 : 3;
  return (
    <span
      className={cx(
        "grid h-20 w-32 gap-1.5 overflow-hidden rounded-lg bg-raised p-2",
        layout === "compact" ? "grid-cols-1" : "grid-cols-3",
        layout === "comparison" && "gap-0",
        layout === "editorial" && "bg-transparent",
      )}
      aria-hidden="true"
    >
      {Array.from({ length: count }, (_, item) => (
        <span
          key={item}
          className={cx(
            "grid content-center gap-1 rounded-md border border-default bg-surface p-1",
            layout === "comparison" && "rounded-none",
            layout === "featured" && item === 1 && "my-[-3px] border-2 border-accent",
            layout === "compact" && "grid-cols-[1fr_.7fr] items-center px-2",
            layout === "editorial" && "rounded-none border-x-0 border-b-0 bg-transparent",
          )}
        >
          <span className="h-1.5 rounded-full bg-primary/70" />
          <span className="h-1 rounded-full bg-accent/60" />
        </span>
      ))}
    </span>
  );
}

function PricingFields({
  draft,
  onChange,
}: {
  draft: PricingDraft;
  onChange: Change;
}) {
  return (
    <>
      <Card as="section" flat>
        <HeroFormHeading icon={<PanelsTopLeft size={17} />}>
          {strings.sitesPricingLayout}
        </HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">
          {strings.sitesPricingLayoutHint}
        </p>
        <HeroOptionRow
          label={strings.sitesPricingLayout}
          value={draft.layout}
          columns={5}
          visual={(layout) => <PricingLayoutVisual layout={layout} />}
          options={[
            ["cards", strings.sitesPricingLayoutCards],
            ["comparison", strings.sitesPricingLayoutComparison],
            ["featured", strings.sitesPricingLayoutFeatured],
            ["compact", strings.sitesPricingLayoutCompact],
            ["editorial", strings.sitesPricingLayoutEditorial],
          ]}
          onChange={(layout) => onChange({ ...draft, layout })}
        />
      </Card>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesFieldIntro}
        value={draft.intro}
        onChange={(intro) => onChange({ ...draft, intro })}
        copyPointer="/intro"
      />
      <ItemsEditor
        addLabel={strings.sitesAddTier}
        items={draft.tiers}
        onChange={(tiers) => onChange({ ...draft, tiers })}
        blank={blankTier}
        render={(tier, update, index) => (
          <>
            <div className={styles.fieldRow}>
              <TextField
                label={strings.sitesFieldTierName}
                value={tier.name}
                onChange={(name) => update({ name })}
              />
              <TextField
                label={strings.sitesFieldPrice}
                value={tier.price}
                onChange={(price) => update({ price })}
              />
            </div>
            <TextField
              label={strings.sitesFieldPeriod}
              value={tier.period}
              onChange={(period) => update({ period })}
            />
            <TextField
              label={strings.sitesFieldTierDescription}
              value={tier.description}
              onChange={(description) => update({ description })}
              copyPointer={`/tiers/${index}/description`}
            />
            <LongTextField
              label={strings.sitesFieldTierFeatures}
              value={tier.featuresText}
              onChange={(featuresText) => update({ featuresText })}
              hint={strings.sitesTierFeaturesHint}
            />
            <LinkFields
              legend={strings.sitesFieldButton}
              value={tier.cta}
              onChange={(patch) => update({ cta: { ...tier.cta, ...patch } })}
            />
            <CheckField
              label={strings.sitesFieldHighlighted}
              checked={tier.highlighted}
              onChange={(highlighted) => update({ highlighted })}
            />
          </>
        )}
      />
    </>
  );
}

function TeamLayoutVisual({ layout }: { layout: TeamDraft["layout"] }) {
  return (
    <span className={cx("grid h-20 w-32 gap-1.5 overflow-hidden rounded-lg bg-raised p-2", layout === "roster" || layout === "compact" ? "grid-cols-1" : "grid-cols-3")} aria-hidden="true">
      {[0, 1, 2].map((item) => (
        <span key={item} className={cx("grid place-items-center gap-1 rounded-md", layout === "cards" && "border border-default bg-surface p-1", layout === "spotlight" && item === 0 && "col-span-3 grid-cols-[2rem_1fr] justify-items-start", (layout === "roster" || layout === "compact") && "grid-cols-[2rem_1fr] justify-items-start")}>
          <span className={cx("size-6 rounded-md bg-accent-soft", layout === "compact" && "rounded-full")} />
          <span className="grid w-full gap-1"><span className="h-1.5 rounded-full bg-primary/70" /><span className="h-1 w-2/3 rounded-full bg-accent/60" /></span>
        </span>
      ))}
    </span>
  );
}

function TeamFields({
  draft,
  onChange,
}: {
  draft: TeamDraft;
  onChange: Change;
}) {
  return (
    <>
      <Card as="section" flat>
        <HeroFormHeading icon={<PanelsTopLeft size={17} />}>{strings.sitesTeamLayout}</HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">{strings.sitesTeamLayoutHint}</p>
        <HeroOptionRow
          label={strings.sitesTeamLayout}
          value={draft.layout}
          columns={5}
          visual={(layout) => <TeamLayoutVisual layout={layout} />}
          options={[["portraits", strings.sitesTeamLayoutPortraits], ["cards", strings.sitesTeamLayoutCards], ["roster", strings.sitesTeamLayoutRoster], ["spotlight", strings.sitesTeamLayoutSpotlight], ["compact", strings.sitesTeamLayoutCompact]]}
          onChange={(layout) => onChange({ ...draft, layout })}
        />
      </Card>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <ItemsEditor
        addLabel={strings.sitesAddMember}
        items={draft.members}
        onChange={(members) => onChange({ ...draft, members })}
        blank={blankMember}
        render={(member, update, index) => (
          <>
            <div className={styles.fieldRow}>
              <TextField
                label={strings.sitesFieldMemberName}
                value={member.name}
                onChange={(name) => update({ name })}
              />
              <TextField
                label={strings.sitesFieldRole}
                value={member.role}
                onChange={(role) => update({ role })}
              />
            </div>
            <ImageFields
              legend={strings.sitesFieldPhoto}
              value={member.photo}
              pointer={`/members/${index}/photo`}
              onChange={(patch) =>
                update({ photo: { ...member.photo, ...patch } })
              }
            />
            <LongTextField
              label={strings.sitesFieldBio}
              value={member.bio}
              onChange={(bio) => update({ bio })}
              copyPointer={`/members/${index}/bio`}
            />
          </>
        )}
      />
    </>
  );
}

function FaqLayoutVisual({ layout }: { layout: FaqDraft["layout"] }) {
  return (
    <span
      className={cx(
        "grid h-20 w-32 gap-1.5 overflow-hidden rounded-lg bg-raised p-2",
        layout === "two_column" || layout === "cards"
          ? "grid-cols-2"
          : "grid-cols-1",
        layout === "editorial" && "bg-transparent px-1",
      )}
      aria-hidden="true"
    >
      {[0, 1, 2, 3].map((item) => (
        <span
          key={item}
          className={cx(
            "grid grid-cols-[1fr_auto] items-center gap-1 rounded-md border border-default bg-surface px-2",
            layout === "divided" && "rounded-none border-x-0 border-b-0 bg-transparent",
            layout === "cards" && "grid-cols-1 p-1.5",
            layout === "editorial" && "rounded-none border-x-0 border-b-0 bg-transparent",
          )}
        >
          <span className="h-1.5 rounded-full bg-primary/70" />
          {layout !== "cards" && <span className="size-1.5 rounded-full bg-accent" />}
        </span>
      ))}
    </span>
  );
}

function FaqFields({ draft, onChange }: { draft: FaqDraft; onChange: Change }) {
  return (
    <>
      <Card as="section" flat>
        <HeroFormHeading icon={<PanelsTopLeft size={17} />}>
          {strings.sitesFaqLayout}
        </HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">
          {strings.sitesFaqLayoutHint}
        </p>
        <HeroOptionRow
          label={strings.sitesFaqLayout}
          value={draft.layout}
          columns={5}
          visual={(layout) => <FaqLayoutVisual layout={layout} />}
          options={[
            ["accordion", strings.sitesFaqLayoutAccordion],
            ["divided", strings.sitesFaqLayoutDivided],
            ["cards", strings.sitesFaqLayoutCards],
            ["two_column", strings.sitesFaqLayoutTwoColumn],
            ["editorial", strings.sitesFaqLayoutEditorial],
          ]}
          onChange={(layout) => onChange({ ...draft, layout })}
        />
      </Card>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <ItemsEditor
        addLabel={strings.sitesAddQuestion}
        items={draft.items}
        onChange={(items) => onChange({ ...draft, items })}
        blank={blankFaqItem}
        render={(item, update, index) => (
          <>
            <TextField
              label={strings.sitesFieldQuestion}
              value={item.question}
              onChange={(question) => update({ question })}
              copyPointer={`/items/${index}/question`}
            />
            <LongTextField
              label={strings.sitesFieldAnswer}
              value={item.answer}
              onChange={(answer) => update({ answer })}
              copyPointer={`/items/${index}/answer`}
            />
          </>
        )}
      />
    </>
  );
}

function CtaLayoutVisual({ layout }: { layout: CtaDraft["layout"] }) {
  return (
    <span className={cx("grid h-20 w-32 items-center gap-2 overflow-hidden rounded-lg bg-accent-soft p-3", layout === "split" && "grid-cols-[1fr_auto]", layout === "banner" && "rounded-none", layout === "card" && "m-1 h-16 w-28 shadow-sm")} aria-hidden="true">
      <span className="grid gap-1"><span className="h-2 rounded-full bg-primary/75" /><span className="h-1.5 w-2/3 rounded-full bg-primary/25" /></span>
      <span className={cx("flex gap-1", layout !== "split" && "justify-center")}><span className="h-3 w-8 rounded-full bg-accent" />{layout === "two_actions" && <span className="h-3 w-8 rounded-full border border-accent" />}</span>
    </span>
  );
}

function CtaFields({ draft, onChange }: { draft: CtaDraft; onChange: Change }) {
  return (
    <>
      <Card as="section" flat>
        <HeroFormHeading icon={<PanelsTopLeft size={17} />}>{strings.sitesCtaLayout}</HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">{strings.sitesCtaLayoutHint}</p>
        <HeroOptionRow label={strings.sitesCtaLayout} value={draft.layout} columns={5} visual={(layout) => <CtaLayoutVisual layout={layout} />} options={[["centered", strings.sitesCtaLayoutCentered], ["split", strings.sitesCtaLayoutSplit], ["banner", strings.sitesCtaLayoutBanner], ["card", strings.sitesCtaLayoutCard], ["two_actions", strings.sitesCtaLayoutTwoActions]]} onChange={(layout) => onChange({ ...draft, layout })} />
      </Card>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesFieldBody}
        value={draft.body}
        onChange={(body) => onChange({ ...draft, body })}
        copyPointer="/body"
      />
      <LinkFields
        legend={strings.sitesFieldButton}
        value={draft.button}
        onChange={(patch) =>
          onChange({ ...draft, button: { ...draft.button, ...patch } })
        }
      />
      {draft.layout === "two_actions" && (
        <LinkFields
          legend={strings.sitesFieldSecondaryButton}
          value={draft.secondary_button}
          onChange={(patch) => onChange({ ...draft, secondary_button: { ...draft.secondary_button, ...patch } })}
        />
      )}
    </>
  );
}

function ContactFormFields({
  draft,
  onChange,
}: {
  draft: ContactFormDraft;
  onChange: Change;
}) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesFieldBody}
        value={draft.body}
        onChange={(body) => onChange({ ...draft, body })}
        copyPointer="/body"
      />
      <TextField
        label={strings.sitesFieldSuccessMessage}
        value={draft.success_message}
        onChange={(success_message) => onChange({ ...draft, success_message })}
        copyPointer="/success_message"
      />
      <p className={styles.hint}>{strings.sitesContactFormHint}</p>
    </>
  );
}

function CollectionFields({
  draft,
  onChange,
}: {
  draft: CollectionDraft;
  onChange: Change;
}) {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [collections, setCollections] = useState<SiteCollection[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void api
      .collections(siteId)
      .then(
        (connected) => {
          if (cancelled) return;
          setCollections(connected);
          setError(null);
        },
        (reason: unknown) => {
          if (!cancelled)
            setError(sitesMessage(reason, strings.sitesCollectionsLoadFailed));
        },
      )
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, siteId]);

  const firstCollectionId = collections[0]?.id;
  useEffect(() => {
    if (draft.collection_id === "" && firstCollectionId !== undefined) {
      onChange({ ...draft, collection_id: firstCollectionId });
    }
  }, [draft, firstCollectionId, onChange]);

  return (
    <>
      <TextField
        label={strings.sitesCollectionSectionHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      {loading ? (
        <div className={styles.collectionFieldLoading} role="status">
          <Spinner size={16} />
          <span>{strings.sitesCollectionsLoading}</span>
        </div>
      ) : collections.length === 0 ? (
        <div className={styles.collectionFieldEmpty}>
          <strong>{strings.sitesCollectionSectionNoConnections}</strong>
          <span>{strings.sitesCollectionSectionNoConnectionsHint}</span>
          <Button
            variant="ghost"
            onClick={() => navigate(`/sites/${siteId}/collections`)}
          >
            {strings.sitesConnectTable}
          </Button>
        </div>
      ) : (
        <label className={styles.field}>
          <span>{strings.sitesCollectionSectionChoose}</span>
          <select
            className={styles.input}
            value={draft.collection_id}
            onChange={(event) =>
              onChange({ ...draft, collection_id: event.target.value })
            }
          >
            {collections.map((collection) => (
              <option key={collection.id} value={collection.id}>
                {collection.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {error !== null && (
        <p className={styles.aiEditError} role="alert">
          {error}
        </p>
      )}
    </>
  );
}

/** Mapping a page to what the site sells: which catalog, and optionally one of
 *  its groups. The prices, names and pictures are not here — they are the
 *  catalog's, frozen into the next publish — so this form asks two questions
 *  and says the two things that surprise people: an edit shows up at the next
 *  publish, and taking orders is a switch on the catalog, not on this page. */
function CatalogFields({
  draft,
  onChange,
}: {
  draft: CatalogDraft;
  onChange: Change;
}) {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [catalogs, setCatalogs] = useState<SiteCatalog[]>([]);
  const [groups, setGroups] = useState<SiteCatalogCategory[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void api
      .catalogs(siteId)
      .then(
        (stored) => {
          if (cancelled) return;
          setCatalogs(stored);
          setError(null);
        },
        (reason: unknown) => {
          if (!cancelled)
            setError(sitesMessage(reason, strings.sitesCatalogsLoadFailed));
        },
      )
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, siteId]);

  const firstCatalogId = catalogs[0]?.id;
  useEffect(() => {
    if (draft.catalog_id === "" && firstCatalogId !== undefined) {
      onChange({ ...draft, catalog_id: firstCatalogId });
    }
  }, [draft, firstCatalogId, onChange]);

  // The groups on offer are the chosen catalog's own. A group is named by its
  // handle in the section, so the list has to come from the server rather than
  // be typed — and a stored handle whose group has since been deleted stays
  // selectable, because silently widening a section to the whole catalog would
  // publish something nobody asked for.
  const chosenId = draft.catalog_id;
  useEffect(() => {
    if (chosenId === "") {
      setGroups([]);
      return;
    }
    let cancelled = false;
    void api.catalog(siteId, chosenId).then(
      (detail) => {
        if (!cancelled) setGroups(detail.categories);
      },
      () => {
        // A catalog that will not load costs the group list, not the form:
        // every group is still a valid answer.
        if (!cancelled) setGroups([]);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, chosenId, siteId]);

  const chosen = catalogs.find((catalog) => catalog.id === draft.catalog_id);
  const missingGroup =
    draft.category !== "" &&
    !groups.some((group) => group.slug === draft.category);

  return (
    <>
      <TextField
        label={strings.sitesCatalogSectionHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      {loading ? (
        <div className={styles.collectionFieldLoading} role="status">
          <Spinner size={16} />
          <span>{strings.sitesCatalogsLoading}</span>
        </div>
      ) : catalogs.length === 0 ? (
        <div className={styles.collectionFieldEmpty}>
          <strong>{strings.sitesCatalogSectionNoCatalogs}</strong>
          <span>{strings.sitesCatalogSectionNoCatalogsHint}</span>
          <Button
            variant="ghost"
            onClick={() => navigate(`/sites/${siteId}/catalogs`)}
          >
            {strings.sitesNewCatalog}
          </Button>
        </div>
      ) : (
        <>
          <Field label={strings.sitesCatalogSectionChoose}>
            <select
              className={styles.input}
              value={draft.catalog_id}
              onChange={(event) =>
                // A group handle belongs to the catalog it came from; changing
                // the catalog drops it rather than carrying a handle that
                // means nothing here.
                onChange({
                  ...draft,
                  catalog_id: event.target.value,
                  category: "",
                })
              }
            >
              {catalogs.map((catalog) => (
                <option key={catalog.id} value={catalog.id}>
                  {catalog.name}
                </option>
              ))}
            </select>
          </Field>
          <Field
            label={strings.sitesCatalogSectionGroup}
            hint={strings.sitesCatalogSectionGroupHint}
          >
            <select
              className={styles.input}
              value={draft.category}
              onChange={(event) =>
                onChange({ ...draft, category: event.target.value })
              }
            >
              <option value="">{strings.sitesCatalogSectionAllGroups}</option>
              {groups.map((group) => (
                <option key={group.id} value={group.slug}>
                  {group.name}
                </option>
              ))}
              {missingGroup && (
                <option value={draft.category}>
                  {strings.sitesCatalogSectionGoneGroup(draft.category)}
                </option>
              )}
            </select>
          </Field>
          <p className={styles.hint}>
            {chosen?.ordersEnabled === true
              ? strings.sitesCatalogSectionOrdersOn
              : strings.sitesCatalogSectionOrdersOff}
          </p>
        </>
      )}
      {error !== null && (
        <p className={styles.aiEditError} role="alert">
          {error}
        </p>
      )}
    </>
  );
}

/** Offering one of the site's booking services on a page. The section holds a
 *  choice and a heading; the length, the week it is open and the questions a
 *  visitor answers are the service's own and are edited on the Bookings screen,
 *  which this form links to rather than duplicating. Two states are said out
 *  loud because a visitor would otherwise be the one to discover them: a site
 *  with no service yet, and a service that is switched off. */
function BookingFields({
  draft,
  onChange,
}: {
  draft: BookingDraft;
  onChange: Change;
}) {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [bookings, setBookings] = useState<SiteBooking[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void api
      .bookings(siteId)
      .then(
        (stored) => {
          if (cancelled) return;
          setBookings(stored);
          setError(null);
        },
        (reason: unknown) => {
          if (!cancelled)
            setError(sitesMessage(reason, strings.sitesBookingsLoadFailed));
        },
      )
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, siteId]);

  const firstBookingId = bookings[0]?.id;
  useEffect(() => {
    if (draft.booking_id === "" && firstBookingId !== undefined) {
      onChange({ ...draft, booking_id: firstBookingId });
    }
  }, [draft, firstBookingId, onChange]);

  const chosen = bookings.find((booking) => booking.id === draft.booking_id);
  return (
    <>
      <TextField
        label={strings.sitesBookingSectionHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      {loading ? (
        <div className={styles.collectionFieldLoading} role="status">
          <Spinner size={16} />
          <span>{strings.sitesBookingsLoading}</span>
        </div>
      ) : bookings.length === 0 ? (
        <div className={styles.collectionFieldEmpty}>
          <strong>{strings.sitesBookingSectionNoServices}</strong>
          <span>{strings.sitesBookingSectionNoServicesHint}</span>
          <Button
            variant="ghost"
            onClick={() => navigate(`/sites/${siteId}/bookings`)}
          >
            {strings.sitesNewBooking}
          </Button>
        </div>
      ) : (
        <>
          <Field label={strings.sitesBookingSectionChoose}>
            <select
              className={styles.input}
              value={draft.booking_id}
              onChange={(event) =>
                onChange({ ...draft, booking_id: event.target.value })
              }
            >
              {bookings.map((booking) => (
                <option key={booking.id} value={booking.id}>
                  {booking.active
                    ? booking.name
                    : strings.sitesBookingSectionOffOption(booking.name)}
                </option>
              ))}
            </select>
          </Field>
          <p className={styles.hint}>
            {chosen === undefined
              ? strings.sitesBookingSectionGone
              : chosen.active
                ? strings.sitesBookingSectionLength(chosen.durationMinutes)
                : strings.sitesBookingSectionOff}
          </p>
        </>
      )}
      {error !== null && (
        <p className={styles.aiEditError} role="alert">
          {error}
        </p>
      )}
    </>
  );
}

/** The ticket shop's door on a page. The section carries the words above the
 *  link and nothing else; the events, their prices and their seats live on
 *  the Tickets screen, which this form links to rather than duplicating. A
 *  site with nothing on sale yet is told so here — a visitor must never be
 *  the one to discover an empty shop. */
function TicketsFields({
  draft,
  onChange,
}: {
  draft: TicketsDraft;
  onChange: Change;
}) {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [onSale, setOnSale] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api.ticketEvents(siteId).then(
      (stored) => {
        if (!cancelled) setOnSale(stored.events.length);
      },
      () => {
        // A list that will not load costs the hint, not the form: the words
        // above the link are editable regardless.
        if (!cancelled) setOnSale(null);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, siteId]);

  return (
    <>
      <TextField
        label={strings.sitesTicketSectionHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesTicketSectionBody}
        value={draft.body}
        onChange={(body) => onChange({ ...draft, body })}
        copyPointer="/body"
      />
      {onSale === 0 ? (
        <div className={styles.collectionFieldEmpty}>
          <strong>{strings.sitesTicketSectionNoEvents}</strong>
          <span>{strings.sitesTicketSectionNoEventsHint}</span>
          <Button
            variant="ghost"
            onClick={() => navigate(`/sites/${siteId}/tickets`)}
          >
            {strings.sitesTickets}
          </Button>
        </div>
      ) : (
        <p className={styles.hint}>
          {onSale === null
            ? strings.sitesTicketSectionHint
            : strings.sitesTicketSectionOnSale(onSale)}
        </p>
      )}
    </>
  );
}

/** The stock shop's door on a page — the tickets form made again for goods
 *  on a shelf. The section carries the words above the link and nothing
 *  else; the shelf, its prices and its stock live on the Shop screen, which
 *  this form links to rather than duplicating. A site with an empty shelf is
 *  told so here — a visitor must never be the one to discover an empty
 *  shop. */
function ShopFields({
  draft,
  onChange,
}: {
  draft: ShopDraft;
  onChange: Change;
}) {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [listed, setListed] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api.shopItems(siteId).then(
      (stored) => {
        if (!cancelled) setListed(stored.items.length);
      },
      () => {
        // A list that will not load costs the hint, not the form: the words
        // above the link are editable regardless.
        if (!cancelled) setListed(null);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, siteId]);

  return (
    <>
      <TextField
        label={strings.sitesShopSectionHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesShopSectionBody}
        value={draft.body}
        onChange={(body) => onChange({ ...draft, body })}
        copyPointer="/body"
      />
      {listed === 0 ? (
        <div className={styles.collectionFieldEmpty}>
          <strong>{strings.sitesShopSectionNoItems}</strong>
          <span>{strings.sitesShopSectionNoItemsHint}</span>
          <Button
            variant="ghost"
            onClick={() => navigate(`/sites/${siteId}/shop`)}
          >
            {strings.sitesShop}
          </Button>
        </div>
      ) : (
        <p className={styles.hint}>
          {listed === null
            ? strings.sitesShopSectionHint
            : strings.sitesShopSectionListed(listed)}
        </p>
      )}
    </>
  );
}

function FooterFields({
  draft,
  onChange,
}: {
  draft: FooterDraft;
  onChange: Change;
}) {
  return (
    <>
      <TextField
        label={strings.sitesFieldFooterText}
        value={draft.text}
        onChange={(text) => onChange({ ...draft, text })}
        autoFocus
        copyPointer="/text"
      />
      <ItemsEditor
        addLabel={strings.sitesAddLink}
        items={draft.links}
        onChange={(links) => onChange({ ...draft, links })}
        blank={blankLink}
        render={(link, update) => <LinkFields value={link} onChange={update} />}
      />
    </>
  );
}

function TransitionVisual({
  effect,
}: {
  effect: TransitionDraft["effect"];
}) {
  return (
    <span className="relative block h-14 w-24 overflow-hidden rounded-lg bg-raised" aria-hidden="true">
      <span className="absolute inset-x-2 top-2 h-3 rounded bg-secondary/15" />
      <span
        className={cx(
          "absolute inset-x-2 bottom-2 h-5 rounded border border-accent/30 bg-accent-soft",
          effect === "fade" && "opacity-60",
          effect === "slide" && "translate-x-2",
          effect === "scale" && "scale-90",
          effect === "reveal" && "[clip-path:inset(0_35%_0_0)]",
        )}
      />
      <span className="absolute left-1/2 top-1/2 h-4 w-px -translate-x-1/2 -translate-y-1/2 bg-accent" />
    </span>
  );
}

function TransitionDirectionVisual({ direction }: { direction: TransitionDraft["direction"] }) {
  const rotation =
    direction === "down" ? "rotate-180" : direction === "left" ? "-rotate-90" : direction === "right" ? "rotate-90" : "";
  return (
    <span className={cx("grid h-12 w-12 place-items-center text-accent", rotation)} aria-hidden="true">
      <span className="text-2xl leading-none">↑</span>
    </span>
  );
}

function TransitionFields({ draft, onChange }: { draft: TransitionDraft; onChange: Change }) {
  return (
    <div className="grid gap-5">
      <Card as="section" flat>
        <HeroFormHeading icon={<Sparkles size={17} />}>
          {strings.sitesTransitionStyle}
        </HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">{strings.sitesTransitionStyleHint}</p>
        <HeroOptionRow
          label={strings.sitesTransitionStyle}
          value={draft.effect}
          columns={4}
          visual={(effect) => <TransitionVisual effect={effect} />}
          options={[
            ["fade", strings.sitesTransitionFade],
            ["slide", strings.sitesTransitionSlide],
            ["scale", strings.sitesTransitionScale],
            ["reveal", strings.sitesTransitionReveal],
          ]}
          onChange={(effect) => onChange({ ...draft, effect })}
        />
      </Card>

      <Card as="section" flat>
        <h3 className="m-0 text-base font-semibold text-primary">{strings.sitesTransitionTiming}</h3>
        <div className="mt-5 grid gap-5 lg:grid-cols-2">
          {draft.effect === "slide" && (
            <HeroOptionRow
              label={strings.sitesTransitionDirection}
              value={draft.direction}
              columns={4}
              visual={(direction) => <TransitionDirectionVisual direction={direction} />}
              options={[
                ["up", strings.sitesTransitionUp],
                ["down", strings.sitesTransitionDown],
                ["left", strings.sitesTransitionLeft],
                ["right", strings.sitesTransitionRight],
              ]}
              onChange={(direction) => onChange({ ...draft, direction })}
            />
          )}
          <HeroOptionRow
            label={strings.sitesHeroAnimationSpeed}
            value={draft.speed}
            visual={(speed) => <HeroControlVisual group="pace" value={speed} />}
            options={[
              ["quick", strings.sitesHeroAnimationQuick],
              ["smooth", strings.sitesHeroAnimationSmooth],
              ["relaxed", strings.sitesHeroAnimationRelaxed],
            ]}
            onChange={(speed) => onChange({ ...draft, speed })}
          />
          <HeroOptionRow
            label={strings.sitesTransitionTrigger}
            value={draft.trigger}
            options={[
              ["early", strings.sitesTransitionEarly],
              ["balanced", strings.sitesTransitionBalanced],
              ["late", strings.sitesTransitionLate],
            ]}
            onChange={(trigger) => onChange({ ...draft, trigger })}
          />
        </div>
        <div className="mt-5 rounded-xl border border-subtle bg-raised p-4">
          <CheckField
            label={strings.sitesTransitionAnimateOut}
            checked={draft.animate_out}
            onChange={(animate_out) => onChange({ ...draft, animate_out })}
          />
          <p className="mb-0 mt-2 text-xs text-secondary">{strings.sitesTransitionAnimateOutHint}</p>
        </div>
      </Card>
    </div>
  );
}

function SectionStyleVisual({ style }: { style: SectionPresentation["layout"] }) {
  return (
    <span className={cx("grid h-16 w-28 gap-1.5 rounded-lg bg-raised p-2", style === "editorial" && "border-l-4 border-accent")} aria-hidden="true">
      <span className="h-2 w-3/5 rounded-full bg-primary/70" />
      <span className={cx("grid grid-cols-2 gap-1", style === "minimal" && "opacity-55")}>
        {[0, 1].map((item) => (
          <span key={item} className={cx("rounded-md bg-accent-soft", style === "cards" && "border border-accent/30 shadow-sm", style === "clean" ? "h-8" : "h-7")} />
        ))}
      </span>
    </span>
  );
}

function SectionEntranceVisual({ entrance }: { entrance: SectionPresentation["entrance"] }) {
  return (
    <span className="relative block h-12 w-20 overflow-hidden" aria-hidden="true">
      <span className="absolute inset-x-1 top-1 h-2 rounded-full bg-secondary/15" />
      <span className={cx(
        "absolute inset-x-1 bottom-1 h-6 rounded-md border border-accent/30 bg-accent-soft",
        entrance === "fade_up" && "translate-y-1 opacity-60",
        entrance === "slide_in" && "translate-x-2 opacity-70",
        entrance === "scale_in" && "scale-90 opacity-70",
        entrance === "reveal" && "[clip-path:inset(0_0_35%_0)]",
      )} />
    </span>
  );
}

function PresentationFields({
  draft,
  onChange,
}: {
  draft: SectionDraft & PresentableDraft;
  onChange: Change;
}) {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const workspaceBrand = readBrandKit();
  const [brandColors, setBrandColors] = useState<BrandColors | null>(null);
  useEffect(() => {
    if (siteId === "") return;
    let current = true;
    void Promise.all([api.site(siteId), api.themePresets()]).then(([site, presets]) => {
      if (!current) return;
      const preset = presets.find((item) => item.id === (site.theme.preset ?? presets[0]?.id));
      if (preset !== undefined) setBrandColors(themeColors(preset, site.theme.colors));
    }).catch(() => undefined);
    return () => { current = false; };
  }, [api, siteId]);

  const colors = brandColors ?? HERO_FALLBACK_COLORS;
  const colorOptions = [
    { value: "background", label: strings.sitesThemeBackgroundColor, swatch: colors.background },
    { value: "text", label: strings.sitesThemeTextColor, swatch: colors.text },
    { value: "border", label: strings.sitesThemeBorderColor, swatch: colors.border },
    { value: "accent_1", label: strings.sitesThemeAccentColor(1), swatch: colors.accent_1 },
    ...(workspaceBrand.secondary === null ? [] : [{ value: "accent_2", label: strings.sitesThemeAccentColor(2), swatch: colors.accent_2 }]),
    ...workspaceBrand.supporting.map((color, index) => ({
      value: `accent_${index + 3}`,
      label: color.name,
      swatch: colors[`accent_${index + 3}` as keyof BrandColors],
    })),
  ];
  const p = draft.presentation;
  const update = (presentation: SectionPresentation) =>
    onChange({ ...draft, presentation } as SectionDraft);
  const hasButtons = ["pricing", "cta", "contact_form", "catalog", "booking", "tickets", "shop"].includes(draft.type);

  return (
    <div className="mt-5 grid gap-5">
      <Card as="section" flat>
        <HeroFormHeading icon={<PanelsTopLeft size={17} />}>
          {strings.sitesSectionDesign}
        </HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">{strings.sitesSectionDesignHint}</p>
        <HeroOptionRow
          label={strings.sitesSectionLayoutStyle}
          value={p.layout}
          columns={4}
          visual={(layout) => <SectionStyleVisual style={layout} />}
          options={[
            ["clean", strings.sitesSectionLayoutClean],
            ["cards", strings.sitesSectionLayoutCards],
            ["minimal", strings.sitesSectionLayoutMinimal],
            ["editorial", strings.sitesSectionLayoutEditorial],
          ]}
          onChange={(layout) => update({ ...p, layout })}
        />
        <div className="mt-5 grid gap-5 lg:grid-cols-3">
          <HeroOptionRow label={strings.sitesSectionSpacing} value={p.spacing} visual={(value) => <HeroControlVisual group="height" value={value === "generous" ? "tall" : value} />} options={[["compact", strings.sitesHeroHeightCompact], ["standard", strings.sitesHeroHeightStandard], ["generous", strings.sitesSectionSpacingGenerous]]} onChange={(spacing) => update({ ...p, spacing })} />
          <HeroOptionRow label={strings.sitesHeroContentWidth} value={p.width} visual={(value) => <HeroControlVisual group="width" value={value} />} options={[["narrow", strings.sitesHeroContentWidthNarrow], ["balanced", strings.sitesHeroContentWidthBalanced], ["wide", strings.sitesHeroContentWidthWide]]} onChange={(width) => update({ ...p, width })} />
          <HeroOptionRow label={strings.sitesHeroAlignment} value={p.alignment} visual={(value) => <HeroControlVisual group="alignment" value={value} />} options={[["left", strings.sitesHeroAlignmentLeft], ["center", strings.sitesHeroAlignmentCenter]]} onChange={(alignment) => update({ ...p, alignment })} />
        </div>
      </Card>

      <Card as="section" flat>
        <HeroFormHeading icon={<Palette size={17} />}>{strings.sitesHeroColors}</HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">{strings.sitesSectionColorsHint}</p>
        <div className="grid gap-5 sm:grid-cols-2">
          <HeroColorSwatches label={strings.sitesSectionBackground} value={p.background} options={colorOptions} onChange={(background) => update({ ...p, background: background as ThemeColorRole })} />
          <HeroColorSwatches label={strings.sitesSectionText} value={p.text} options={colorOptions} onChange={(text) => update({ ...p, text: text as ThemeColorRole })} />
        </div>
        {hasButtons && (
          <div className="mt-5 grid gap-5 rounded-xl border border-subtle bg-raised p-4 sm:grid-cols-2 lg:grid-cols-4">
            <HeroColorSwatches label={strings.sitesHeroPrimaryButtonColor} value={p.button} options={colorOptions} onChange={(button) => update({ ...p, button: button as ThemeColorRole })} />
            <HeroColorSwatches label={strings.sitesHeroPrimaryButtonText} value={p.button_text ?? "auto"} options={colorOptions} automatic onChange={(button_text) => update({ ...p, button_text: button_text === "auto" ? undefined : button_text as ThemeColorRole })} />
            <HeroColorSwatches label={strings.sitesHeroHoverBackground} value={p.button_hover} options={colorOptions} onChange={(button_hover) => update({ ...p, button_hover: button_hover as ThemeColorRole })} />
            <HeroColorSwatches label={strings.sitesHeroHoverText} value={p.button_hover_text ?? "auto"} options={colorOptions} automatic onChange={(button_hover_text) => update({ ...p, button_hover_text: button_hover_text === "auto" ? undefined : button_hover_text as ThemeColorRole })} />
          </div>
        )}
      </Card>

      <Card as="section" flat>
        <HeroFormHeading icon={<Sparkles size={17} />}>{strings.sitesHeroAnimation}</HeroFormHeading>
        <p className="mb-5 mt-2 text-sm text-secondary">{strings.sitesSectionAnimationHint}</p>
        <HeroOptionRow
          label={strings.sitesSectionEntrance}
          value={p.entrance}
          columns={5}
          visual={(entrance) => <SectionEntranceVisual entrance={entrance} />}
          options={[["none", strings.sitesHeroAnimationNone], ["fade_up", strings.sitesHeroTextFadeUp], ["slide_in", strings.sitesHeroTextSlideIn], ["scale_in", strings.sitesTransitionScale], ["reveal", strings.sitesTransitionReveal]]}
          onChange={(entrance) => update({ ...p, entrance })}
        />
        {p.entrance !== "none" && <div className="mt-5 max-w-xl"><HeroOptionRow label={strings.sitesHeroAnimationSpeed} value={p.speed} visual={(speed) => <HeroControlVisual group="pace" value={speed} />} options={[["quick", strings.sitesHeroAnimationQuick], ["smooth", strings.sitesHeroAnimationSmooth], ["relaxed", strings.sitesHeroAnimationRelaxed]]} onChange={(speed) => update({ ...p, speed })} /></div>}
        <div className="mt-4 flex justify-end"><Button variant="ghost" size="sm" onClick={() => update(DEFAULT_SECTION_PRESENTATION)}>{strings.sitesSectionUseDefaults}</Button></div>
      </Card>
    </div>
  );
}

function SectionSpecificFields({
  draft,
  onChange,
  currentPage,
  currentSections,
}: {
  draft: SectionDraft;
  onChange: Change;
  currentPage?: SitePage | undefined;
  currentSections: Section[];
}) {
  switch (draft.type) {
    case "nav":
      return (
        <NavFields
          draft={draft}
          onChange={onChange}
          currentPage={currentPage}
          currentSections={currentSections}
        />
      );
    case "hero":
      return <HeroFields draft={draft} onChange={onChange} />;
    case "features":
      return <FeaturesFields draft={draft} onChange={onChange} />;
    case "text_image":
      return <TextImageFields draft={draft} onChange={onChange} />;
    case "gallery":
      return <GalleryFields draft={draft} onChange={onChange} />;
    case "testimonials":
      return <TestimonialsFields draft={draft} onChange={onChange} />;
    case "pricing":
      return <PricingFields draft={draft} onChange={onChange} />;
    case "team":
      return <TeamFields draft={draft} onChange={onChange} />;
    case "faq":
      return <FaqFields draft={draft} onChange={onChange} />;
    case "cta":
      return <CtaFields draft={draft} onChange={onChange} />;
    case "contact_form":
      return <ContactFormFields draft={draft} onChange={onChange} />;
    case "collection":
      return <CollectionFields draft={draft} onChange={onChange} />;
    case "catalog":
      return <CatalogFields draft={draft} onChange={onChange} />;
    case "booking":
      return <BookingFields draft={draft} onChange={onChange} />;
    case "tickets":
      return <TicketsFields draft={draft} onChange={onChange} />;
    case "shop":
      return <ShopFields draft={draft} onChange={onChange} />;
    case "transition":
      return <TransitionFields draft={draft} onChange={onChange} />;
    case "custom_code":
      // No copy tools anywhere in this form: the assistant refuses to write
      // or change code by name (`alo-ai`'s sites module), so offering the
      // affordance would only produce a refusal.
      return <CustomCodeFields draft={draft} onChange={onChange} />;
    case "footer":
      return <FooterFields draft={draft} onChange={onChange} />;
  }
}

export function SectionFormFields(props: {
  draft: SectionDraft;
  onChange: Change;
  currentPage?: SitePage | undefined;
  currentSections: Section[];
}) {
  const { draft, onChange } = props;
  return (
    <>
      <SectionSpecificFields {...props} />
      {"presentation" in draft && (
        <PresentationFields draft={draft as SectionDraft & PresentableDraft} onChange={onChange} />
      )}
    </>
  );
}

// ---- the dialog -------------------------------------------------------------

/** The three sections that name something else of the site cannot be saved
 *  until they name it: a section pointing at nothing would be a page that
 *  publishes an empty hole, or refuses the publish outright. Every other kind
 *  is ruled on by the server alone. */
function canSubmit(draft: SectionDraft): boolean {
  switch (draft.type) {
    case "collection":
      return draft.collection_id !== "";
    case "catalog":
      return draft.catalog_id !== "";
    case "booking":
      return draft.booking_id !== "";
    default:
      return true;
  }
}

/** The section prop form: fresh for a kind picked in the picker, prefilled
 *  when editing an existing section. Saving hands the wire section up; the
 *  caller talks to the server and feeds any refusal back through `error`,
 *  so the dialog stays open with everything the user typed. */
export function SectionFormDialog({
  kind,
  initial,
  busy,
  error,
  onClose,
  onSave,
  copyContext,
  currentPage,
  currentSections = [],
}: {
  kind: SectionKind;
  /** The stored section when editing; absent when adding. */
  initial?: Section | undefined;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onSave: (section: Section) => void;
  /** Present only for a stored section. New sections have no stable page
   *  target yet, so their copy remains directly editable until first save. */
  copyContext?: CopyContextValue | undefined;
  /** The open page and its stack let navigation destinations include content
   *  sections without another request or a second source of truth. */
  currentPage?: SitePage | undefined;
  currentSections?: Section[] | undefined;
}) {
  const [draft, setDraft] = useState<SectionDraft>(() =>
    toDraft(kind, initial),
  );
  const label = kindLabel(kind);
  return (
    <DialogFrame
      Icon={kind === "nav" ? PanelTop : Blocks}
      title={
        initial === undefined
          ? strings.sitesAddSectionTitle(label)
          : strings.sitesEditSectionTitle(label)
      }
      subtitle={kindDescription(kind)}
      error={error}
      busy={busy}
      canSubmit={canSubmit(draft)}
      submitLabel={strings.sitesSaveSection}
      wide={kind === "nav" || kind === "hero" || kind === "transition"}
      onClose={onClose}
      onSubmit={() => onSave(toSection(draft))}
    >
      <CopyContext.Provider value={copyContext ?? null}>
        <SectionFormFields
          draft={draft}
          onChange={setDraft}
          currentPage={currentPage}
          currentSections={currentSections}
        />
      </CopyContext.Provider>
    </DialogFrame>
  );
}
