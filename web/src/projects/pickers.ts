// The engagements a screen outside Projects has to name.
//
// Its own file for the reason Billing's `pickers.ts` is: a module that only
// needs "the id and the name of each project" should not import the Projects
// API client, its error type and its whole record shape to get them. This is
// the narrow thing that crosses the module line, and it is the only one — the
// hours, the budgets and the weeks stay behind `ProjectsModule`.
//
// The first caller is the expense claim form (alo Finance, B4.13a): a lunch
// bought for a client belongs to that engagement, and the profitability report
// (B3.08) is the reason it is worth attaching.
import { useEffect, useState } from "react";

import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import type { Project } from "./types";

/** Every project this caller can see, for a picker. */
export function useProjects(): { projects: Project[]; error: string | null } {
  const api = useProjectsApi();
  const [projects, setProjects] = useState<Project[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const list = await api.projects();
        if (live) {
          setProjects(list);
          setError(null);
        }
      } catch (err) {
        if (live) setError(projectsMessage(err, strings.projectsLoadFailed));
      }
    })();
    return () => {
      live = false;
    };
  }, [api]);

  return { projects, error };
}
