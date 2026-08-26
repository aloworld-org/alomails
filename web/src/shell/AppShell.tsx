// The one-product frame: the constant left rail beside the active module,
// which renders through <Outlet>. An optional right-hand panel hosts the
// ＋New and ✦AI surfaces (placeholders this pass). Nothing here is
// mail-specific — every module lives inside this same frame.
import { useEffect, useState } from "react";
import { Outlet, useLocation } from "react-router-dom";
import { Sparkles, X } from "lucide-react";

import { strings } from "../i18n";
import { IconButton } from "../ds";
import { Rail } from "./Rail";
import { ComingSoon } from "./ComingSoon";
import { surface } from "../product";
import { recordAppVisit } from "./appUsage";
import { SearchOverlay } from "./SearchOverlay";

export function AppShell() {
  const location = useLocation();
  useEffect(() => {
    const id = surface.modules.find(
      (module) =>
        location.pathname === module.path ||
        location.pathname.startsWith(`${module.path}/`),
    )?.id;
    if (id !== undefined) recordAppVisit(id);
  }, [location.pathname]);

  const [aiOpen, setAiOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);

  // Ctrl/Cmd-K opens workspace search from anywhere.
  useEffect(() => {
    function key(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen(true);
      }
    }
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, []);

  return (
    <div className="relative grid h-dvh min-h-0 overflow-hidden grid-cols-[var(--rail-width)_minmax(0,1fr)_auto] grid-rows-1 [grid-template-areas:'rail_main_panel'] max-md:grid-cols-1 max-md:grid-rows-[minmax(0,1fr)_auto] max-md:[grid-template-areas:'main'_'rail']">
      <Rail className="[grid-area:rail]" onAskAi={() => setAiOpen(true)} />

      <main className="min-h-0 min-w-0 overflow-hidden bg-app [grid-area:main]">
        <Outlet />
      </main>

      {searchOpen && <SearchOverlay onClose={() => setSearchOpen(false)} />}

      {aiOpen && (
        <aside className="flex w-[340px] flex-col border-l border-subtle bg-surface [grid-area:panel] max-md:fixed max-md:inset-0 max-md:z-[900] max-md:w-auto max-md:border-l-0" aria-label={strings.moduleAi}>
          <div className="flex justify-end p-3">
            <IconButton
              size="sm"
              label="Close"
              icon={<X />}
              onClick={() => setAiOpen(false)}
            />
          </div>
          <ComingSoon title={strings.moduleAi} Icon={Sparkles} />
        </aside>
      )}
    </div>
  );
}
