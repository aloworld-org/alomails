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
import styles from "./AppShell.module.css";

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
    <div className={styles.shell}>
      <Rail onAskAi={() => setAiOpen(true)} />

      <main className={styles.main}>
        <Outlet />
      </main>

      {searchOpen && <SearchOverlay onClose={() => setSearchOpen(false)} />}

      {aiOpen && (
        <aside className={styles.panel} aria-label={strings.moduleAi}>
          <div className={styles.panelHead}>
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
