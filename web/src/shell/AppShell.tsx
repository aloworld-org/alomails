// The one-product frame: the constant left rail beside the active module,
// which renders through <Outlet>. An optional right-hand panel hosts the
// ＋New and ✦AI surfaces (placeholders this pass). Nothing here is
// mail-specific — every module lives inside this same frame.
import { useState } from "react";
import { Outlet } from "react-router-dom";
import { Sparkles, X } from "lucide-react";

import { strings } from "../i18n";
import { IconButton } from "../ds";
import { Rail } from "./Rail";
import { ComingSoon } from "./ComingSoon";
import styles from "./AppShell.module.css";

export function AppShell() {
  const [aiOpen, setAiOpen] = useState(false);

  return (
    <div className={styles.shell}>
      <Rail onAskAi={() => setAiOpen(true)} />

      <main className={styles.main}>
        <Outlet />
      </main>

      {aiOpen && (
        <aside className={styles.panel} aria-label={strings.moduleAi}>
          <div className={styles.panelHead}>
            <IconButton size="sm" label="Close" icon={<X />} onClick={() => setAiOpen(false)} />
          </div>
          <ComingSoon title={strings.moduleAi} Icon={Sparkles} />
        </aside>
      )}
    </div>
  );
}
