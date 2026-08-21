// A site's blog desk. alo Docs is the authoring surface; Sites keeps the
// public metadata and publish state. Creating an article therefore makes the
// source document and links it as one operation, while every existing row
// offers its edit action directly on the surface.
import { useCallback, useEffect, useState } from "react";
import {
  ArrowLeft,
  ExternalLink,
  FilePenLine,
  Newspaper,
  PencilLine,
  Send,
  Undo2,
} from "lucide-react";
import { Link, useNavigate, useParams } from "react-router-dom";

import { Button } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap/useJmapClient";
import { sitesMessage, useSitesApi } from "./api";
import { EmptyState, ErrorBanner } from "./parts";
import { PostPublishDialog } from "./PostPublishDialog";
import type { SiteDetail, SitePost } from "./types";

const styles = {
  page: "mx-auto flex w-full max-w-7xl flex-col gap-6 px-4 py-6 sm:px-6 lg:px-8",
  header: "flex flex-wrap items-start gap-4 border-b border-subtle pb-5",
  backLink:
    "inline-flex min-h-10 items-center gap-2 rounded-xl px-3 text-sm font-semibold text-secondary no-underline transition hover:bg-muted hover:text-primary",
  siteHead: "min-w-0 flex-1",
  title: "text-2xl font-semibold tracking-tight text-primary",
  postSiteName: "mt-1 block truncate text-sm text-secondary",
  headerActions: "flex flex-wrap items-center gap-2 sm:ml-auto",
  postSkeletons:
    "grid gap-3 rounded-2xl border border-subtle bg-surface p-5 shadow-sm [&_span]:h-20 [&_span]:animate-pulse [&_span]:rounded-xl [&_span]:bg-muted",
  tableWrap:
    "overflow-x-auto rounded-2xl border border-subtle bg-surface shadow-sm",
  table:
    "w-full min-w-[52rem] border-collapse text-sm [&_th]:border-b [&_th]:border-subtle [&_th]:bg-muted/60 [&_th]:px-5 [&_th]:py-3 [&_th]:text-left [&_th]:text-xs [&_th]:font-semibold [&_th]:uppercase [&_th]:tracking-wide [&_th]:text-secondary [&_td]:border-b [&_td]:border-subtle [&_td]:px-5 [&_td]:py-4 [&_td]:align-middle [&_tbody_tr:last-child_td]:border-b-0 [&_tbody_tr]:transition [&_tbody_tr:hover]:bg-muted/35",
  postTitle: "block max-w-xl truncate font-semibold text-primary",
  postExcerpt: "mt-1 block max-w-xl truncate text-sm leading-5 text-secondary",
  chip: "inline-flex min-h-7 items-center rounded-full bg-muted px-2.5 text-xs font-semibold text-secondary",
  chipLive: "bg-success/10 text-success ring-1 ring-inset ring-success/20",
  postDate: "whitespace-nowrap text-secondary",
  postActionCell: "min-w-[22rem]",
  postActions: "flex flex-wrap items-center justify-end gap-2",
};

const updated = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

/** A private draft URL derived from the source document id. The real public
 *  slug is chosen in the publish flow; this only gives the stored draft a
 *  collision-resistant, valid placeholder in the meantime. */
function draftSlug(documentId: string): string {
  const safe = documentId
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 70)
    .replace(/-+$/g, "");
  return `draft-${safe || "article"}`;
}

export function PostsView() {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const jmap = useJmapClient();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [posts, setPosts] = useState<SitePost[]>([]);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [editingPost, setEditingPost] = useState<SitePost | null>(null);
  const [unpublishingId, setUnpublishingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [detail, rows] = await Promise.all([
        api.site(siteId),
        api.posts(siteId),
      ]);
      setSite(detail);
      setPosts(rows);
      setError(null);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesPostsLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  function edit(post: SitePost) {
    navigate(`/drive?open=${encodeURIComponent(post.docNodeId)}`);
  }

  async function writeInDocs() {
    if (creating) return;
    setCreating(true);
    setError(null);
    let documentId: string | null = null;
    try {
      documentId = await jmap.driveCreateDoc(
        null,
        null,
        strings.sitesUntitledArticle,
      );
      await api.createPost(siteId, {
        docNodeId: documentId,
        slug: draftSlug(documentId),
        title: strings.sitesUntitledArticle,
        excerpt: "",
      });
      navigate(`/drive?open=${encodeURIComponent(documentId)}`);
    } catch (err) {
      // If Sites refused the link, keep Drive tidy without destroying data:
      // the brand-new blank document goes to Trash and remains recoverable.
      if (documentId !== null) {
        try {
          await jmap.driveTrashNode(documentId);
        } catch {
          // The original, actionable Sites reason is the one the user needs.
        }
      }
      setError(sitesMessage(err, strings.sitesPostCreateFailed));
      setCreating(false);
    }
  }

  async function unpublish(post: SitePost) {
    setUnpublishingId(post.id);
    setError(null);
    try {
      await api.unpublishPost(siteId, post.id);
      await load();
    } catch (err) {
      setError(sitesMessage(err, strings.sitesPostUnpublishFailed));
    } finally {
      setUnpublishingId(null);
    }
  }

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesBackToWebsite}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesPosts}</h1>
          {site !== null && (
            <span className={styles.postSiteName}>{site.name}</span>
          )}
        </div>
        <div className={styles.headerActions}>
          <Button
            icon={
              <FilePenLine size="var(--icon-size-inline)" aria-hidden="true" />
            }
            disabled={creating}
            onClick={() => void writeInDocs()}
          >
            {creating ? strings.sitesOpeningDocs : strings.sitesWriteInDocs}
          </Button>
        </div>
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {loading ? (
        <div
          className={styles.postSkeletons}
          role="status"
          aria-label={strings.sitesLoadingPosts}
        >
          <span />
          <span />
          <span />
        </div>
      ) : posts.length === 0 ? (
        <EmptyState
          Icon={Newspaper}
          title={strings.sitesNoPostsTitle}
          body={strings.sitesNoPostsBody}
          cta={strings.sitesWriteInDocs}
          onCta={() => void writeInDocs()}
        />
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.sitesColArticle}</th>
                <th scope="col">{strings.sitesColStatus}</th>
                <th scope="col">{strings.sitesColUpdated}</th>
                <th scope="col" aria-label={strings.sitesColActions} />
              </tr>
            </thead>
            <tbody>
              {posts.map((post) => (
                <tr key={post.id}>
                  <td>
                    <strong className={styles.postTitle}>{post.title}</strong>
                    {post.excerpt !== "" && (
                      <span className={styles.postExcerpt}>{post.excerpt}</span>
                    )}
                  </td>
                  <td>
                    <span
                      className={
                        post.status === "published"
                          ? `${styles.chip} ${styles.chipLive}`
                          : styles.chip
                      }
                    >
                      {post.status === "published"
                        ? strings.sitesPostStatusPublished
                        : strings.sitesPostStatusDraft}
                    </span>
                  </td>
                  <td className={styles.postDate}>
                    {updated.format(new Date(post.updatedAt))}
                  </td>
                  <td className={styles.postActionCell}>
                    <div className={styles.postActions}>
                      <Button
                        variant="ghost"
                        icon={
                          <ExternalLink
                            size="var(--icon-size-inline)"
                            aria-hidden="true"
                          />
                        }
                        onClick={() => edit(post)}
                      >
                        {strings.sitesEditInDocs}
                      </Button>
                      <Button
                        variant={post.status === "draft" ? "primary" : "ghost"}
                        icon={
                          post.status === "draft" ? (
                            <Send
                              size="var(--icon-size-inline)"
                              aria-hidden="true"
                            />
                          ) : (
                            <PencilLine
                              size="var(--icon-size-inline)"
                              aria-hidden="true"
                            />
                          )
                        }
                        onClick={() => setEditingPost(post)}
                      >
                        {post.status === "draft"
                          ? strings.sitesPublishArticle
                          : strings.sitesEditArticleDetails}
                      </Button>
                      {post.status === "published" && (
                        <Button
                          variant="ghost"
                          icon={
                            <Undo2
                              size="var(--icon-size-inline)"
                              aria-hidden="true"
                            />
                          }
                          disabled={unpublishingId === post.id}
                          onClick={() => void unpublish(post)}
                        >
                          {unpublishingId === post.id
                            ? strings.sitesUnpublishingArticle
                            : strings.sitesUnpublishArticle}
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {editingPost !== null && (
        <PostPublishDialog
          siteId={siteId}
          post={editingPost}
          onClose={() => setEditingPost(null)}
          onSaved={() => {
            setEditingPost(null);
            void load();
          }}
        />
      )}
    </div>
  );
}
