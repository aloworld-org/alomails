// The left rail — the constant of the one-product frame (Figma app shell).
// Top: the mark and ＋New. Middle: one labelled item per registered module,
// the active one highlighted. Bottom: ✦AI and the account menu. It never
// scrolls and never changes between modules; only the panel to its right does.
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Grip, Sparkles } from "lucide-react";
import { NavLink } from "react-router-dom";

import { strings } from "../i18n";
import { cx } from "../ds";
import { surface } from "../product";
import { mostUsedApps } from "./appUsage";
import { isModuleAllowed, useDeniedModules } from "./moduleAccess";
import { Logo } from "./Logo";
import { UserMenu } from "./UserMenu";
import styles from "./Rail.module.css";

interface RailProps {
  /** ✦AI action (assistant panel — placeholder until the AI layer). */
  onAskAi: () => void;
}

export function Rail({ onAskAi }: RailProps) {
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
  const [open, setOpen] = useState(false);
  const launcherTriggerRef = useRef<HTMLLIElement>(null);
  const launcherPanelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      const target = event.target as Node;
      if (
        !launcherTriggerRef.current?.contains(target) &&
        !launcherPanelRef.current?.contains(target)
      ) {
        setOpen(false);
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  const favoriteModules = favorites.flatMap((id) => {
    const module = apps.find((app) => app.id === id);
    return module === undefined ? [] : [module];
  });

  return (
    <nav className={styles.rail} aria-label={strings.appName}>
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
        <li ref={launcherTriggerRef} className={styles.launcherAnchor}>
          <button
            type="button"
            className={cx(styles.item, open && styles.active)}
            onClick={() => {
              setOpen((current) => !current);
            }}
            aria-expanded={open}
            aria-haspopup="dialog"
            title={strings.appLauncher}
          >
            <Grip strokeWidth={2} />
            <span className={styles.label}>{strings.appLauncher}</span>
          </button>
          {open &&
            createPortal(
              <div
                ref={launcherPanelRef}
                className={styles.launcher}
                role="dialog"
                aria-label={strings.appLauncher}
              >
                <div className={styles.launcherHead}>
                  <strong>{strings.appLauncherFavorites}</strong>
                  {/* Said once, because a list that rearranges itself is
                      confusing until somebody knows why it does. */}
                  <span className={styles.launcherHint}>
                    {strings.appLauncherAutoHint}
                  </span>
                </div>
                <div className={styles.launcherScroll}>
                  <div className={styles.favoriteCard}>
                    <div className={styles.appGrid}>
                      {favoriteModules.map((app) => (
                        <NavLink
                          key={app.id}
                          to={app.path}
                          className={cx(styles.appTile)}
                          onClick={() => setOpen(false)}
                        >
                          <app.Icon />
                          <span>{app.label}</span>
                        </NavLink>
                      ))}
                    </div>
                  </div>
                  <h3>{strings.appLauncherAll}</h3>
                  <div className={styles.appGrid}>
                    {apps.map((app) => (
                      <NavLink
                        key={app.id}
                        to={app.path}
                        className={cx(styles.appTile)}
                        onClick={() => setOpen(false)}
                      >
                        <app.Icon />
                        <span>{app.label}</span>
                      </NavLink>
                    ))}
                  </div>
                </div>
              </div>,
              document.body,
            )}
        </li>
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
