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
      className="overflow-x-auto rounded-xl border border-subtle bg-surface p-1 shadow-sm"
      aria-label={strings.sitesWebsiteNavigation}
    >
      <div className="flex min-w-max items-center gap-1" role="tablist">
        {items.map(({ id, label, Icon }) => {
          if (id === "collaborators" && !showCollaborators) return null;
          const selected = active === id;
          return (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={selected}
              className={`inline-flex min-h-10 cursor-pointer items-center gap-2 rounded-lg border-0 px-3.5 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30 ${
                selected
                  ? "!bg-accent-soft font-semibold !text-accent"
                  : "!bg-transparent font-medium !text-text-secondary hover:!bg-surface-raised hover:!text-text-primary"
              }`}
              onClick={() => onSelect(id)}
            >
              <Icon size={16} aria-hidden="true" />
              {label()}
            </button>
          );
        })}
      </div>
    </nav>
  );
}
