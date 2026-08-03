// The one-product frame: the constant left rail beside the active module,
// which renders through <Outlet>. An optional right-hand panel hosts the
// ＋New and ✦AI surfaces (placeholders this pass). Nothing here is
// mail-specific — every module lives inside this same frame.
import { useState } from "react";
import { Outlet } from "react-router-dom";
import { Plus, Sparkles, X } from "lucide-react";

import { strings } from "../i18n";
import { IconButton } from "../ds";
import { Rail } from "./Rail";
import { ComingSoon } from "./ComingSoon";
import styles from "./AppShell.module.css";

type Panel = "new" | "ai" | null;

export function AppShell() {
  const [panel, setPanel] = useState<Panel>(null);

  return (
    <div className={styles.shell}>
      <Rail onNew={() => setPanel("new")} onAskAi={() => setPanel("ai")} />

      <main className={styles.main}>
        <Outlet />
      </main>

      {panel !== null && (
        <aside className={styles.panel} aria-label={panel === "ai" ? strings.moduleAi : strings.newButton}>
          <div className={styles.panelHead}>
            <IconButton
              size="sm"
              label="Close"
              icon={<X />}
              onClick={() => setPanel(null)}
            />
          </div>
          {panel === "ai" ? (
            <ComingSoon title={strings.moduleAi} Icon={Sparkles} />
          ) : (
            <ComingSoon title={strings.newButton} Icon={Plus} />
          )}
        </aside>
      )}
    </div>
  );
}
