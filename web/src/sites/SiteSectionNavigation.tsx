import { FileText, Languages, Rocket, Rows3, Users } from "lucide-react";

import {
  ModuleNavigation,
  moduleNavigationItemClassName,
} from "../ds";
import { strings } from "../i18n";

export type SiteWorkspace =
  | "pages"
  | "publishing"
  | "languages"
  | "collaborators"
  | "tools";

const items = [
  { id: "pages", label: () => strings.sitesPages, Icon: FileText },
  { id: "publishing", label: () => strings.sitesPublishing, Icon: Rocket },
  { id: "languages", label: () => strings.sitesLanguages, Icon: Languages },
  {
    id: "collaborators",
    label: () => strings.sitesCollaborators,
    Icon: Users,
  },
  { id: "tools", label: () => strings.sitesSiteTools, Icon: Rows3 },
] satisfies Array<{
  id: SiteWorkspace;
  label: () => string;
  Icon: typeof FileText;
}>;

export function SiteSectionNavigation({
  active,
  showCollaborators,
  onSelect,
}: {
  active: SiteWorkspace;
  showCollaborators: boolean;
  onSelect: (workspace: SiteWorkspace) => void;
}) {
  return (
    <ModuleNavigation label={strings.sitesWebsiteNavigation}>
      <div className="contents" role="tablist">
        {items.map(({ id, label, Icon }) => {
          if (id === "collaborators" && !showCollaborators) return null;
          const selected = active === id;
          return (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={selected}
              className={moduleNavigationItemClassName(selected)}
              onClick={() => onSelect(id)}
            >
              <Icon className="size-4" aria-hidden="true" />
              {label()}
            </button>
          );
        })}
      </div>
    </ModuleNavigation>
  );
}
