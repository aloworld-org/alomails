import { useCallback, useEffect, useState } from "react";
import { CalendarDays, FolderKanban, ListChecks, MessageSquare, Sparkles } from "lucide-react";
import { Link } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import { ProjectSetupDialog } from "./ProjectSetupDialog";
import type { ProjectSetup } from "./types";

export function ProjectResources({ projectId, projectName }: { projectId: string; projectName: string }) {
  const api = useProjectsApi();
  const [setup, setSetup] = useState<ProjectSetup | null>(null);
  const [editing, setEditing] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setSetup(await api.projectSetup(projectId));
      setError(null);
    } catch (reason) {
      setError(projectsMessage(reason, strings.projectsSetupLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, projectId]);

  useEffect(() => void load(), [load]);

  return (
    <div className="shrink-0 px-8 pt-3 max-sm:px-4">
      <section className="rounded-xl border border-subtle bg-surface p-4 shadow-sm">
        <div className="flex flex-wrap items-center gap-3">
          <span className="rounded-lg bg-accent-soft p-2 text-accent"><Sparkles size={18} /></span>
          <div className="min-w-0 flex-1">
            <h3 className="m-0 text-sm font-semibold text-primary">{strings.projectsResources}</h3>
            <p className="mb-0 mt-1 text-xs text-secondary">{strings.projectsResourcesSubtitle}</p>
          </div>
          {loading ? <Spinner size={16} /> : <Button variant="secondary" onClick={() => setEditing(true)}>{setup === null ? strings.projectsSetupAction : strings.projectsSetupAddAction}</Button>}
        </div>
        {error !== null && <p className="mb-0 mt-3 text-sm text-danger" role="alert">{error}</p>}
        {setup !== null && (
          <div className="mt-3 grid grid-cols-4 gap-2 max-lg:grid-cols-2 max-sm:grid-cols-1">
            {setup.spaceId !== null && <Link className="flex min-h-11 items-center gap-2 rounded-lg border border-subtle px-3 text-sm font-medium !text-primary !no-underline hover:bg-raised" to={`/drive?space=${encodeURIComponent(setup.spaceId)}`}><FolderKanban size={16} />{strings.projectsFiles}</Link>}
            {setup.chatChannelId !== null && <Link className="flex min-h-11 items-center gap-2 rounded-lg border border-subtle px-3 text-sm font-medium !text-primary !no-underline hover:bg-raised" to={`/chat?channel=${encodeURIComponent(setup.chatChannelId)}`}><MessageSquare size={16} />{strings.projectsChatRoom}</Link>}
            {setup.kickoffEventId !== null && <Link className="flex min-h-11 items-center gap-2 rounded-lg border border-subtle px-3 text-sm font-medium !text-primary !no-underline hover:bg-raised" to="/agenda"><CalendarDays size={16} />{strings.projectsKickoffMeeting}</Link>}
            {setup.starterTaskIds.length > 0 && <Link className="flex min-h-11 items-center gap-2 rounded-lg border border-subtle px-3 text-sm font-medium !text-primary !no-underline hover:bg-raised" to={`/projects/${encodeURIComponent(projectId)}/list`}><ListChecks size={16} />{strings.projectsStarterTasks(setup.starterTaskIds.length)}</Link>}
          </div>
        )}
      </section>
      {editing && <ProjectSetupDialog projectId={projectId} projectName={projectName} onClose={() => setEditing(false)} onSaved={(saved) => { setSetup(saved); setEditing(false); }} />}
    </div>
  );
}
