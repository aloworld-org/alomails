import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Grip } from "lucide-react";
import { NavLink } from "react-router-dom";

import { cx } from "../ds";
import { strings } from "../i18n";
import type { ProductModule } from "../product";
import styles from "./Rail.module.css";

interface AppLauncherProps {
  apps: ProductModule[];
  favoriteModules: ProductModule[];
}

export function AppLauncher({ apps, favoriteModules }: AppLauncherProps) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLLIElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const favoriteIds = new Set(favoriteModules.map((app) => app.id));
  const remainingApps = apps.filter((app) => !favoriteIds.has(app.id));

  useEffect(() => {
    if (!open) return;

    const closeOutside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (
        !triggerRef.current?.contains(target) &&
        !panelRef.current?.contains(target)
      ) {
        setOpen(false);
      }
    };
    const closeWithEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };

    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeWithEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeWithEscape);
    };
  }, [open]);

  const tile = (app: ProductModule) => (
    <NavLink
      key={app.id}
      to={app.path}
      className={({ isActive }) =>
        cx(
          "group flex min-h-20 min-w-0 flex-col items-center justify-center gap-2 rounded-xl border border-transparent px-2 py-3 text-xs font-medium text-secondary transition-colors duration-150 hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/15",
          isActive && "border-accent/20 bg-accent-soft text-accent",
        )
      }
      onClick={() => setOpen(false)}
    >
      <span className="grid size-10 place-items-center rounded-xl bg-surface text-primary transition-colors duration-150 group-hover:text-accent group-aria-[current=page]:text-accent">
        <app.Icon className="size-5" strokeWidth={1.75} />
      </span>
      <span className="max-w-full truncate">{app.label}</span>
    </NavLink>
  );

  return (
    <li ref={triggerRef}>
      <button
        type="button"
        className={cx(styles.item, open && styles.active)}
        onClick={() => setOpen((current) => !current)}
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-label={strings.appLauncher}
        title={strings.appLauncher}
      >
        <Grip strokeWidth={2} />
        <span className={styles.label}>{strings.appLauncher}</span>
      </button>

      {open &&
        createPortal(
          <div
            ref={panelRef}
            className="fixed left-[calc(var(--rail-width)+10px)] top-[76px] z-[1000] flex max-h-[calc(100dvh-92px)] w-[min(360px,calc(100vw-var(--rail-width)-28px))] flex-col overflow-hidden rounded-3xl border border-default bg-raised text-primary shadow-lg max-md:bottom-[60px] max-md:left-3 max-md:top-auto max-md:max-h-[60dvh] max-md:w-[calc(100vw-24px)]"
            role="dialog"
            aria-label={strings.appLauncher}
          >
            <header className="px-5 pb-4 pt-5">
              <h2 className="text-lg font-semibold tracking-tight text-primary">
                {strings.appLauncherFavorites}
              </h2>
              <p className="mt-1 max-w-xs text-sm leading-5 text-secondary">
                {strings.appLauncherAutoHint}
              </p>
            </header>

            <div className="min-h-0 overflow-y-auto px-4 pb-5">
              <section aria-label={strings.appLauncherFavorites}>
                <div className="grid grid-cols-3 gap-2">
                  {favoriteModules.map(tile)}
                </div>
              </section>

              {remainingApps.length > 0 && (
                <section className="mt-5 border-t border-subtle pt-5">
                  <h3 className="mb-2 px-2 text-xs font-semibold tracking-[0.08em] text-tertiary">
                    {strings.appLauncherMore}
                  </h3>
                  <div className="grid grid-cols-3 gap-2">
                    {remainingApps.map(tile)}
                  </div>
                </section>
              )}
            </div>
          </div>,
          document.body,
        )}
    </li>
  );
}
