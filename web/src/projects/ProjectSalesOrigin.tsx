import { useEffect, useState } from "react";
import { ExternalLink, Handshake } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { IconButton } from "../ds";
import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import type { ProjectSalesOrigin as SalesOrigin } from "./types";

/** The durable reverse link from delivery back to the opportunity that won it. */
export function ProjectSalesOrigin({ projectId }: { projectId: string }) {
  const api = useProjectsApi();
  const navigate = useNavigate();
  const [origin, setOrigin] = useState<SalesOrigin | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void api
      .salesOrigin(projectId)
      .then((relationship) => {
        if (!live) return;
        setOrigin(relationship);
        setError(null);
      })
      .catch((reason) => {
        if (live) setError(projectsMessage(reason, strings.projectsSalesOriginLoadFailed));
      });
    return () => {
      live = false;
    };
  }, [api, projectId]);

  if (origin === null && error === null) return null;

  return (
    <div className="shrink-0 px-8 pt-4 max-sm:px-4">
      <section className="flex items-center gap-3 rounded-xl border border-subtle bg-surface px-4 py-3 shadow-sm">
        <span className="rounded-lg bg-accent-soft p-2 text-accent" aria-hidden="true">
          <Handshake size={18} />
        </span>
        <div className="min-w-0 flex-1">
          <p className="m-0 text-xs font-semibold uppercase tracking-wide text-tertiary">
            {strings.projectsSalesOrigin}
          </p>
          {origin !== null ? (
            <p className="mb-0 mt-1 truncate text-sm font-semibold text-primary">
              {origin.dealTitle}
            </p>
          ) : (
            <p className="mb-0 mt-1 text-sm text-danger" role="alert">
              {error}
            </p>
          )}
        </div>
        {origin !== null && (
          <IconButton
            label={strings.projectsOpenSalesOrigin}
            icon={<ExternalLink size={17} />}
            onClick={() =>
              navigate(`/crm/board?deal=${encodeURIComponent(origin.dealId)}`)
            }
          />
        )}
      </section>
    </div>
  );
}
