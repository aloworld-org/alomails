// The create-page form: a title, the path it answers on, and the home flag.
// The empty path is the home page's spelling — the server enforces that pair
// (and the slug rules, and one-home-per-site), so the form only carries the
// hint and shows the refusal sentence when a rule is broken.
import { useState } from "react";
import { FileText } from "lucide-react";

import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { DialogFrame, Field } from "./parts";
import type { SitePage } from "./types";
import styles from "./SitesModule.module.css";

export function NewPageDialog({
  siteId,
  firstPage,
  onClose,
  onCreated,
}: {
  siteId: string;
  /** True when the site has no pages yet — the first page defaults to home. */
  firstPage: boolean;
  onClose: () => void;
  onCreated: (page: SitePage) => void;
}) {
  const api = useSitesApi();
  const [title, setTitle] = useState("");
  const [slug, setSlug] = useState("");
  const [home, setHome] = useState(firstPage);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    setBusy(true);
    try {
      onCreated(await api.createPage(siteId, { title: title.trim(), slug: slug.trim(), home }));
    } catch (err) {
      setError(sitesMessage(err, strings.sitesSaveFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={FileText}
      title={strings.sitesNewPageTitle}
      subtitle={strings.sitesNewPageSubtitle}
      error={error}
      busy={busy}
      canSubmit={title.trim() !== ""}
      submitLabel={strings.sitesCreatePage}
      onClose={onClose}
      onSubmit={() => void submit()}
    >
      <Field label={strings.sitesFieldPageTitle}>
        <input
          className={styles.input}
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          autoFocus
        />
      </Field>
      <Field label={strings.sitesFieldSlug} hint={strings.sitesSlugHint}>
        <input
          className={styles.input}
          value={slug}
          onChange={(e) => setSlug(e.target.value)}
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
        />
      </Field>
      <label className={styles.toggle}>
        <input type="checkbox" checked={home} onChange={(e) => setHome(e.target.checked)} />
        {strings.sitesFieldHome}
      </label>
    </DialogFrame>
  );
}
