// Per-page search and sharing metadata. The action is visible on the editor
// surface (not menu-gated); the dialog keeps the two optional overrides and a
// live recognition-first preview together. Blank values deliberately mean
// "use the automatic page/site defaults" rather than an invalid empty tag.
import { useState } from "react";
import { SearchCheck } from "lucide-react";

import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { DialogFrame, Field } from "./parts";
import type { SitePageDetail } from "./types";
import styles from "./SitesModule.module.css";

export function PageSeoDialog({
  siteId,
  page,
  onClose,
  onSaved,
}: {
  siteId: string;
  page: SitePageDetail;
  onClose: () => void;
  onSaved: (seoTitle: string | null, seoDescription: string | null) => void;
}) {
  const api = useSitesApi();
  const [title, setTitle] = useState(page.seoTitle ?? "");
  const [description, setDescription] = useState(page.seoDescription ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const cleanTitle = title.trim();
  const cleanDescription = description.trim();
  const previewTitle = cleanTitle || page.title;
  const previewDescription = cleanDescription || strings.sitesSeoDescriptionDefault;

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await api.setPageSeo(siteId, page.id, cleanTitle, cleanDescription);
      onSaved(cleanTitle || null, cleanDescription || null);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesSeoSaveFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={SearchCheck}
      title={strings.sitesSeoTitle}
      subtitle={strings.sitesSeoSubtitle}
      error={error}
      busy={busy}
      canSubmit
      submitLabel={strings.sitesSeoSave}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <div className={styles.seoPreview} aria-label={strings.sitesSeoPreview}>
        <span className={styles.seoPreviewPath}>/{page.slug}</span>
        <strong className={styles.seoPreviewTitle}>{previewTitle}</strong>
        <span className={styles.seoPreviewDescription}>{previewDescription}</span>
      </div>
      <Field label={strings.sitesSeoFieldTitle} hint={strings.sitesSeoTitleHint}>
        <input
          className={styles.input}
          value={title}
          placeholder={page.title}
          autoFocus
          onChange={(event) => setTitle(event.target.value)}
        />
      </Field>
      <Field
        label={strings.sitesSeoFieldDescription}
        hint={strings.sitesSeoDescriptionHint}
      >
        <textarea
          className={`${styles.input} ${styles.textarea}`}
          value={description}
          onChange={(event) => setDescription(event.target.value)}
        />
      </Field>
      <p className={styles.seoImageHint}>{strings.sitesSeoImageHint}</p>
    </DialogFrame>
  );
}
