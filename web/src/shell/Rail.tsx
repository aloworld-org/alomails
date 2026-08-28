// The left rail — the constant of the one-product frame (Figma app shell).
// Top: the mark and ＋New. Middle: one labelled item per registered module,
// the active one highlighted. Bottom: ✦AI and the account menu. It never
// scrolls and never changes between modules; only the panel to its right does.
import { Sparkles } from "lucide-react";
import { NavLink } from "react-router-dom";

import { strings } from "../i18n";
import { cx } from "../ds";
import { surface } from "../product";
import { mostUsedApps } from "./appUsage";
import { AppLauncher } from "./AppLauncher";
import { isModuleAllowed, useDeniedModules } from "./moduleAccess";
import { Logo } from "./Logo";
import { UserMenu } from "./UserMenu";
import styles from "./Rail.module.css";

interface RailProps {
  /** ✦AI action (assistant panel — placeholder until the AI layer). */
  onAskAi: () => void;
  className?: string;
}

export function Rail({ onAskAi, className }: RailProps) {
  // Apps this person was actually given (migration 0208). Filtered before the
  // favourites are computed, so a switched-off app cannot occupy one of the
  // six shortcut slots and cannot be "recently used" into the top of the rail.
  const denied = useDeniedModules();
  const apps = surface.modules.filter(
    (module) => module.id !== "home" && isModuleAllowed(denied, module.id),
  );
  const home = surface.modules.find((module) => module.id === "home");
  // Derived, never stored: the six you have been using, recomputed whenever
  // the rail mounts. There is no saved list because there is nothing to save —
  // a shortcut list you have to maintain is a chore, and this one maintains
  // itself.
  const used = mostUsedApps(6).filter((id) => apps.some((a) => a.id === id));
  const favorites = [
    ...used,
    ...apps.map((m) => m.id).filter((id) => !used.includes(id)),
  ].slice(0, 6);
  const favoriteModules = favorites.flatMap((id) => {
    const module = apps.find((app) => app.id === id);
    return module === undefined ? [] : [module];
  });

  return (
    <nav className={cx(styles.rail, className)} aria-label={strings.appName}>
      <div className={styles.top}>
        <NavLink
          to="/mail"
          className={cx(styles.logoLink)}
          aria-label={strings.appName}
        >
          <Logo size={40} />
        </NavLink>
      </div>

      <ul className={styles.modules}>
        {home !== undefined && (
          <li>
            <NavLink
              to={home.path}
              className={({ isActive }) =>
                cx(styles.item, isActive && styles.active)
              }
              title={home.label}
            >
              <home.Icon strokeWidth={1.75} />
              <span className={styles.label}>{home.label}</span>
            </NavLink>
          </li>
        )}
        <AppLauncher apps={apps} favoriteModules={favoriteModules} />
        {favoriteModules.map((m) => (
          <li key={m.id}>
            <NavLink
              to={m.path}
              className={({ isActive }) =>
                cx(styles.item, isActive && styles.active)
              }
              title={m.label}
            >
              <m.Icon strokeWidth={1.75} />
              <span className={styles.label}>{m.label}</span>
            </NavLink>
          </li>
        ))}
      </ul>

      <div className={styles.bottom}>
        {/* Whatever this product says must be visible from every module — the
            running timer, in the workspace. Each renders nothing when it has
            nothing to say; the rail knows what none of them are about. */}
        {(surface.railWidgets ?? []).map((widget) => (
          <widget.Widget key={widget.id} />
        ))}
        <button type="button" className={styles.item} onClick={onAskAi}>
          <Sparkles strokeWidth={1.75} />
          <span className={styles.label}>{strings.moduleAi}</span>
        </button>
        <UserMenu />
      </div>
    </nav>
  );
}
