// Public metadata for one alo Docs-backed article. Drafts publish after the
// metadata write succeeds; already-published articles save in place. The
// server remains the authority on slug, text, image, and tenancy rules.
import { useRef, useState } from "react";
import { Image, Upload } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap/useJmapClient";
import { sitesMessage, useSitesApi } from "./api";
import { DialogFrame, Field } from "./parts";
import type { SitePost } from "./types";
import styles from "./SitesModule.module.css";

export function PostPublishDialog({
  siteId,
  post,
  onClose,
  onSaved,
}: {
  siteId: string;
  post: SitePost;
  onClose: () => void;
  onSaved: () => void;
}) {
  const api = useSitesApi();
  const jmap = useJmapClient();
  const fileInput = useRef<HTMLInputElement>(null);
  const [title, setTitle] = useState(post.title);
  const [slug, setSlug] = useState(post.slug.startsWith("draft-") ? "" : post.slug);
  const [excerpt, setExcerpt] = useState(post.excerpt);
  const [cover, setCover] = useState<string | null>(post.coverBlobId);
  const [busy, setBusy] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const publishing = post.status === "draft";

  async function uploadCover(file: File) {
    setUploading(true);
    setError(null);
    try {
      const uploaded = await jmap.driveUploadBlob(null, null, file);
      setCover(uploaded.blobId);
    } catch {
      setError(strings.sitesPostCoverUploadFailed);
    } finally {
      setUploading(false);
    }
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await api.updatePost(siteId, post.id, {
        title: title.trim(),
        slug: slug.trim(),
        excerpt: excerpt.trim(),
        coverBlobId: cover,
      });
      if (publishing) await api.publishPost(siteId, post.id);
      onSaved();
    } catch (err) {
      setError(sitesMessage(err, strings.sitesPostSaveFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Image}
      title={publishing ? strings.sitesPublishArticleTitle : strings.sitesEditArticleTitle}
      subtitle={
        publishing ? strings.sitesPublishArticleSubtitle : strings.sitesEditArticleSubtitle
      }
      error={error}
      busy={busy || uploading}
      canSubmit={title.trim() !== "" && slug.trim() !== ""}
      submitLabel={publishing ? strings.sitesPublishArticle : strings.sitesSaveArticle}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={strings.sitesFieldPostTitle}>
        <input
          className={styles.input}
          value={title}
          autoFocus
          onChange={(event) => setTitle(event.target.value)}
        />
      </Field>
      <Field label={strings.sitesFieldPostSlug} hint={strings.sitesPostSlugHint}>
        <input
          className={`${styles.input} ${styles.mono}`}
          value={slug}
          placeholder={strings.sitesPostSlugPlaceholder}
          onChange={(event) => setSlug(event.target.value)}
        />
      </Field>
      <Field label={strings.sitesFieldPostExcerpt} hint={strings.sitesPostExcerptHint}>
        <textarea
          className={`${styles.input} ${styles.textarea}`}
          value={excerpt}
          onChange={(event) => setExcerpt(event.target.value)}
        />
      </Field>
      <div className={styles.postCoverField}>
        <div>
          <span className={styles.label}>{strings.sitesFieldPostCover}</span>
          <p className={styles.hint}>{strings.sitesPostCoverHint}</p>
        </div>
        <div className={styles.postCoverActions}>
          <span className={styles.themeSlotState}>
            {cover === null ? strings.sitesPostNoCover : strings.sitesPostCoverAdded}
          </span>
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            aria-label={strings.sitesFieldPostCover}
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              event.target.value = "";
              if (file !== undefined) void uploadCover(file);
            }}
          />
          <Button
            variant="ghost"
            icon={<Upload size="var(--icon-size-inline)" aria-hidden="true" />}
            disabled={busy || uploading}
            onClick={() => fileInput.current?.click()}
          >
            {uploading
              ? strings.sitesUploadingPostCover
              : cover === null
                ? strings.sitesAddPostCover
                : strings.sitesReplacePostCover}
          </Button>
          {cover !== null && (
            <Button
              variant="ghost"
              disabled={busy || uploading}
              onClick={() => setCover(null)}
            >
              {strings.sitesRemovePostCover}
            </Button>
          )}
        </div>
      </div>
    </DialogFrame>
  );
}
