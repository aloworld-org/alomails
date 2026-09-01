import {
  FileText,
  Gauge,
  Languages,
  PanelBottom,
  PanelTop,
  Rocket,
  Settings,
  Users,
} from "lucide-react";

import { ModuleNavigation, moduleNavigationItemClassName } from "../ds";
import { strings } from "../i18n";

export type SiteWorkspace =
  | "overview"
  | "pages"
  | "navigation"
  | "footer"
  | "publishing"
  | "languages"
  | "collaborators"
  | "tools";

const items = [
  { id: "overview", label: () => strings.sitesOverview, Icon: Gauge },
  { id: "pages", label: () => strings.sitesPages, Icon: FileText },
  { id: "navigation", label: () => strings.sitesNavigation, Icon: PanelTop },
  { id: "footer", label: () => strings.sitesSectionFooter, Icon: PanelBottom },
  { id: "languages", label: () => strings.sitesLanguages, Icon: Languages },
  { id: "publishing", label: () => strings.sitesPublishing, Icon: Rocket },
  {
    id: "collaborators",
    label: () => strings.sitesCollaborators,
    Icon: Users,
  },
  { id: "tools", label: () => strings.sitesSiteSettings, Icon: Settings },
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
    <ModuleNavigation label={strings.sitesWebsiteNavigation} className="pb-3">
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
