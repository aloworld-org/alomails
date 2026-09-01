import { useCallback, useEffect, useState } from "react";
import { Check, PanelBottom, PanelTop } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { SectionFormFields } from "./SectionForm";
import { toDraft, toSection } from "./sectionDrafts";
import type { SectionDraft } from "./sectionDrafts";
import type { Section } from "./sections";
import type { SitePage, SitePageDetail } from "./types";

type StructureKind = "nav" | "footer";

export function SiteStructureEditor({
  siteId,
  page,
  kind,
}: {
  siteId: string;
  page?: SitePage | undefined;
  kind: StructureKind;
}) {
  const api = useSitesApi();
  const [detail, setDetail] = useState<SitePageDetail | null>(null);
  const [sections, setSections] = useState<Section[]>([]);
  const [draft, setDraft] = useState<SectionDraft | null>(null);
  const [loading, setLoading] = useState(page !== undefined);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (page === undefined) {
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const loaded = await api.page(siteId, page.id);
      const loadedSections = loaded.sections.sections;
      setDetail(loaded);
      setSections(loadedSections);
      setDraft(
        toDraft(
          kind,
          loadedSections.find((section) => section.type === kind),
        ),
      );
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesPageLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, kind, page, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function save() {
    if (page === undefined || draft === null) return;
    setSaving(true);
    setSaved(false);
    setError(null);
    try {
      const section = toSection(draft);
      const index = sections.findIndex((item) => item.type === kind);
      const envelope =
        index >= 0
          ? await api.updateSection(siteId, page.id, index, section)
          : await api.addSection(
              siteId,
              page.id,
              section,
              kind === "nav" ? 0 : sections.length,
            );
      setSections(envelope.sections);
      setDraft(
        toDraft(
          kind,
          envelope.sections.find((item) => item.type === kind),
        ),
      );
      setSaved(true);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesSaveFailed));
    } finally {
      setSaving(false);
    }
  }

  const navigation = kind === "nav";
  const title = navigation
    ? strings.sitesNavigation
    : strings.sitesSectionFooter;
  const description = navigation
    ? strings.sitesSectionNavDesc
    : strings.sitesSectionFooterDesc;
  const Icon = navigation ? PanelTop : PanelBottom;

  return (
    <section
      className="overflow-visible rounded-2xl border border-subtle bg-surface shadow-sm"
      aria-labelledby={`site-${kind}-title`}
    >
      <header className="flex min-h-20 flex-wrap items-center gap-4 px-5 py-4 sm:px-6">
        <span
          className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent"
          aria-hidden="true"
        >
          <Icon className="size-5" />
        </span>
        <span className="min-w-0 flex-1">
          <h2
            id={`site-${kind}-title`}
            className="m-0 text-lg font-semibold text-primary"
          >
            {title}
          </h2>
          <span className="mt-1 block text-sm text-secondary">
            {description}
          </span>
        </span>
        {saved && (
          <span
            className="inline-flex items-center gap-2 text-sm font-medium text-success"
            role="status"
          >
            <Check className="size-4" aria-hidden="true" />
            {strings.sitesSectionSaved}
          </span>
        )}
        <Button disabled={saving || draft === null} onClick={() => void save()}>
          {saving ? <Spinner size={16} /> : strings.sitesSaveSection}
        </Button>
      </header>

      <div className="border-t border-subtle px-5 py-5 sm:px-6 sm:py-6">
        {loading && (
          <div className="grid min-h-48 place-items-center">
            <Spinner />
          </div>
        )}
        {!loading && page === undefined && (
          <div className="py-12 text-center">
            <h3 className="m-0 text-base font-semibold text-primary">
              {strings.sitesNoPagesTitle}
            </h3>
            <p className="mx-auto mb-0 mt-2 max-w-lg text-sm text-secondary">
              {strings.sitesNoPagesBody}
            </p>
          </div>
        )}
        {!loading && error !== null && (
          <p
            className="m-0 rounded-xl bg-danger-tint px-4 py-3 text-sm text-primary"
            role="alert"
          >
            {error}
          </p>
        )}
        {!loading && detail !== null && draft !== null && (
          <SectionFormFields
            draft={draft}
            onChange={(next) => {
              setDraft(next);
              setSaved(false);
            }}
            currentPage={detail}
            currentSections={sections}
          />
        )}
      </div>
    </section>
  );
}
