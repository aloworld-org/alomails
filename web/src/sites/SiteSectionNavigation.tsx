import { FileText, Languages, Rocket, Rows3, Users } from "lucide-react";

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
    <nav
      className="flex min-w-0 gap-2 overflow-x-auto"
      aria-label={strings.sitesWebsiteNavigation}
    >
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
              className={`inline-flex min-h-11 shrink-0 cursor-pointer items-center gap-2.5 rounded-xl border-0 px-4 py-2.5 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent ${
                selected
                  ? "!bg-accent-soft font-semibold !text-accent shadow-sm ring-1 ring-inset ring-accent/10"
                  : "!bg-transparent font-medium !text-secondary hover:!bg-raised hover:!text-primary"
              }`}
              onClick={() => onSelect(id)}
            >
              <Icon className="size-4" aria-hidden="true" />
              {label()}
            </button>
          );
        })}
      </div>
    </nav>
  );
}
