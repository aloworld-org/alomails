// The Sites module (alo Sites, ADR 0036, wave S1) — the workspace surface
// over the `/sites/*` edit API: the site list, and one page per site. It is
// mounted at `/sites/*` by the product surface, so paths below are relative
// and a deep link (`/sites/{id}`) survives a page reload.
import { Navigate, Route, Routes } from "react-router-dom";

import { PageEditorView } from "./PageEditorView";
import { CollectionsView } from "./CollectionsView";
import { PostsView } from "./PostsView";
import { SiteView } from "./SiteView";
import { AnalyticsView } from "./AnalyticsView";
import { HistoryView } from "./HistoryView";
import { SitesListView } from "./SitesListView";
import { SubmissionsView } from "./SubmissionsView";
import styles from "./SitesModule.module.css";

export function SitesModule() {
  return (
    <div className={styles.sites}>
      <Routes>
        <Route index element={<SitesListView />} />
        <Route path=":siteId" element={<SiteView />} />
        <Route path=":siteId/analytics" element={<AnalyticsView />} />
        <Route path=":siteId/collections" element={<CollectionsView />} />
        <Route path=":siteId/history" element={<HistoryView />} />
        <Route path=":siteId/submissions" element={<SubmissionsView />} />
        <Route path=":siteId/posts" element={<PostsView />} />
        <Route path=":siteId/pages/:pageId" element={<PageEditorView />} />
        {/* An unknown deeper path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to="/sites" replace />} />
      </Routes>
    </div>
  );
}
