import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { strings } from "../i18n";
import type { ProductModule } from "../product";
import { AppTile } from "./AppTile";

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

  return (
    <li ref={triggerRef} className="w-full">
      <button
        type="button"
        className={`flex w-full flex-col items-center gap-[3px] rounded-xl px-0 py-2 text-on-rail-muted transition-colors duration-150 hover:bg-rail-hover hover:text-on-rail focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-[#E76F51]/20 ${
          open ? "bg-rail-active text-on-rail" : ""
        }`}
        onClick={() => setOpen((current) => !current)}
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-label={strings.appLauncher}
        title={strings.appLauncher}
      >
        <svg
          className="size-[22px]"
          viewBox="0 0 24 24"
          fill="currentColor"
          aria-hidden="true"
        >
          {[5, 12, 19].flatMap((y) =>
            [5, 12, 19].map((x) => (
              <circle key={`${x}-${y}`} cx={x} cy={y} r="1.65" />
            )),
          )}
        </svg>
        <span className="text-[10px] font-medium tracking-[0.01em]">
          {strings.appLauncher}
        </span>
      </button>

      {open &&
        createPortal(
          <div
            ref={panelRef}
            className="fixed left-[calc(var(--rail-width)+10px)] top-[76px] z-[1000] flex max-h-[calc(100dvh-92px)] w-[min(380px,calc(100vw-var(--rail-width)-28px))] flex-col overflow-hidden rounded-[28px] border border-[#CBD5E1]/65 bg-white text-[#102A43] shadow-[0_12px_36px_rgba(16,42,67,0.08)] max-md:bottom-[60px] max-md:left-3 max-md:top-auto max-md:max-h-[60dvh] max-md:w-[calc(100vw-24px)]"
            role="dialog"
            aria-label={strings.appLauncher}
          >
            <header className="px-6 pb-5 pt-6">
              <h2 className="text-lg font-semibold tracking-tight text-[#102A43]">
                {strings.appLauncherFavorites}
              </h2>
              <p className="mt-1.5 max-w-xs text-sm leading-5 text-slate-500">
                {strings.appLauncherAutoHint}
              </p>
            </header>

            <div className="min-h-0 overflow-y-auto px-6 pb-6">
              <section aria-label={strings.appLauncherFavorites}>
                <div className="grid grid-cols-3 gap-x-3 gap-y-4">
                  {favoriteModules.map((app) => (
                    <AppTile
                      key={app.id}
                      app={app}
                      onSelect={() => setOpen(false)}
                    />
                  ))}
                </div>
              </section>

              {remainingApps.length > 0 && (
                <section className="mt-6 border-t border-[#CBD5E1]/55 pt-6">
                  <h3 className="mb-3 text-sm font-semibold text-slate-500">
                    {strings.appLauncherMore}
                  </h3>
                  <div className="grid grid-cols-3 gap-x-3 gap-y-4">
                    {remainingApps.map((app) => (
                      <AppTile
                        key={app.id}
                        app={app}
                        onSelect={() => setOpen(false)}
                      />
                    ))}
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
