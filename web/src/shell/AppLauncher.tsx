import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";

import { strings } from "../i18n";
import { MODAL_BACKDROP_CLASS } from "../ds";
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
        className={`group flex w-full flex-col items-center gap-[3px] rounded-xl px-0 py-2 transition-colors duration-150 hover:bg-[#2B3439] focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-[#E76F51]/20 ${
          open ? "bg-[#343D42]" : ""
        }`}
        onClick={() => setOpen((current) => !current)}
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-label={strings.appLauncher}
        title={strings.appLauncher}
      >
        <span
          className="grid size-[22px] grid-cols-3 place-items-center"
          aria-hidden="true"
        >
          {Array.from({ length: 9 }, (_, index) => (
            <span
              key={index}
              data-launcher-dot
              className={`size-1 rounded-full transition-colors duration-150 ${
                open
                  ? "bg-white"
                  : "bg-[#D7DEE2] group-hover:bg-white group-focus-visible:bg-white"
              }`}
            />
          ))}
        </span>
        <span
          data-launcher-label
          className={`text-[10px] font-medium tracking-[0.01em] transition-colors duration-150 ${
            open
              ? "text-white"
              : "text-[#D7DEE2] group-hover:text-white group-focus-visible:text-white"
          }`}
        >
          {strings.appLauncher}
        </span>
      </button>

      {open &&
        createPortal(
          <>
            <div
              data-app-launcher-backdrop
              className={`fixed inset-0 z-[1190] cursor-default bg-overlay ${MODAL_BACKDROP_CLASS}`}
              onPointerDown={() => setOpen(false)}
              aria-hidden="true"
            />
            <div
              ref={panelRef}
              className="fixed bottom-4 left-[82px] top-20 z-[1200] isolate flex w-[380px] max-w-[calc(100vw-98px)] flex-col overflow-hidden rounded-3xl border border-[#CBD5E1]/70 bg-[#FFFEFC] text-[#102A43] shadow-[0_18px_48px_rgba(16,42,67,0.16)] max-md:bottom-[76px] max-md:left-3 max-md:top-3 max-md:w-[calc(100vw-24px)] max-md:max-w-none"
              role="dialog"
              aria-modal="true"
              aria-label={strings.appLauncher}
            >
              <header className="flex shrink-0 items-start justify-between gap-4 border-b border-[#CBD5E1]/45 bg-[#FFFEFC] px-6 py-5">
                <div className="min-w-0">
                  <h2 className="text-lg font-semibold tracking-tight text-[#102A43]">
                    {strings.appLauncherFavorites}
                  </h2>
                  <p className="mt-1 max-w-[240px] text-sm leading-5 text-slate-500">
                    {strings.appLauncherAutoHint}
                  </p>
                </div>
                <button
                  type="button"
                  className="grid size-10 shrink-0 place-items-center rounded-xl text-[#102A43] transition-colors duration-150 hover:bg-[#E76F51]/10 hover:text-[#E76F51] focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-[#E76F51]/15"
                  onClick={() => setOpen(false)}
                  aria-label={strings.close}
                >
                  <X className="size-5" aria-hidden="true" />
                </button>
              </header>

              <div className="min-h-0 flex-1 overflow-y-auto bg-[#FFFEFC] px-6 py-5">
                <section aria-label={strings.appLauncherFavorites}>
                  <div className="grid grid-cols-2 gap-3">
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
                  <section className="mt-6 border-t border-[#CBD5E1]/55 pt-5">
                    <h3 className="mb-3 text-sm font-semibold text-slate-500">
                      {strings.appLauncherMore}
                    </h3>
                    <div className="grid grid-cols-2 gap-3">
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
            </div>
          </>,
          document.body,
        )}
    </li>
  );
}
