// The left rail — the constant of the one-product frame (Figma app shell).
// Top: the mark and ＋New. Middle: one labelled item per registered module,
// the active one highlighted. Bottom: ✦AI and the account menu. It never
// scrolls and never changes between modules; only the panel to its right does.
import { Plus, Sparkles } from "lucide-react";
import { NavLink } from "react-router-dom";

import { strings } from "../i18n";
import { cx } from "../ds";
import { Logo } from "./Logo";
import { UserMenu } from "./UserMenu";
import { modules } from "./moduleRegistry";
import styles from "./Rail.module.css";

interface RailProps {
  /** ＋New action (compose in Mail; contextual later). */
  onNew: () => void;
  /** ✦AI action (assistant panel — placeholder until the AI layer). */
  onAskAi: () => void;
}

export function Rail({ onNew, onAskAi }: RailProps) {
  return (
    <nav className={styles.rail} aria-label={strings.appName}>
      <div className={styles.top}>
        <NavLink to="/mail" className={cx(styles.logoLink)} aria-label={strings.appName}>
          <Logo size={40} />
        </NavLink>
        <button type="button" className={styles.newButton} onClick={onNew} aria-label={strings.newButton}>
          <Plus />
        </button>
      </div>

      <ul className={styles.modules}>
        {modules.map((m) => (
          <li key={m.id}>
            <NavLink
              to={m.path}
              className={({ isActive }) => cx(styles.item, isActive && styles.active)}
              title={m.label}
            >
              <m.Icon strokeWidth={1.75} />
              <span className={styles.label}>{m.label}</span>
            </NavLink>
          </li>
        ))}
      </ul>

      <div className={styles.bottom}>
        <button type="button" className={styles.item} onClick={onAskAi}>
          <Sparkles strokeWidth={1.75} />
          <span className={styles.label}>{strings.moduleAi}</span>
        </button>
        <UserMenu />
      </div>
    </nav>
  );
}
