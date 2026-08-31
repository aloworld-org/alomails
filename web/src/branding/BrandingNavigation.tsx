import { BookOpen, Eye, Fingerprint, Shapes } from "lucide-react";
import { NavLink } from "react-router-dom";

import { ModuleNavigation, moduleNavigationItemClassName } from "../ds";
import { strings } from "../i18n";

const AREAS = [
  { path: "foundation", label: () => strings.brandingFoundationNav, Icon: Fingerprint },
  { path: "visual-identity", label: () => strings.brandingVisualIdentityNav, Icon: Shapes },
  { path: "applications", label: () => strings.brandingApplicationsNav, Icon: Eye },
  { path: "guidelines", label: () => strings.brandingGuidelinesNav, Icon: BookOpen },
] as const;

export function BrandingNavigation() {
  return (
    <div className="shrink-0 border-b border-subtle bg-surface px-4 sm:px-6 lg:px-8 print:hidden">
      <ModuleNavigation className="mx-auto w-full max-w-[94rem] py-3" label={strings.brandingNavLabel}>
        {AREAS.map(({ path, label, Icon }) => (
          <NavLink key={path} to={`/branding/${path}`} className={({ isActive }) => moduleNavigationItemClassName(isActive)}>
            <Icon aria-hidden="true" />
            {label()}
          </NavLink>
        ))}
      </ModuleNavigation>
    </div>
  );
}
