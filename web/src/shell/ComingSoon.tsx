// A calm placeholder for parts of the workspace not yet built. Used both as a
// whole-module surface (Agenda/Chat/…) and inside the ＋New / ✦AI panels, so
// the suite feels present and honest while only Mail is live.
import type { LucideIcon } from "lucide-react";

import { strings } from "../i18n";
import styles from "./ComingSoon.module.css";

interface ComingSoonProps {
  title?: string;
  body?: string;
  Icon?: LucideIcon;
}

export function ComingSoon({ title, body, Icon }: ComingSoonProps) {
  return (
    <div className={styles.wrap}>
      {Icon !== undefined && (
        // Decoration, not a badge: the heading under it says what this is, and
        // a plate holding a module's own glyph has nothing to announce.
        <div className={styles.plate} aria-hidden="true">
          <Icon strokeWidth={1.5} />
        </div>
      )}
      <h2 className={styles.title}>{title ?? strings.comingSoonTitle}</h2>
      <p className={styles.body}>{body ?? strings.comingSoonBody}</p>
    </div>
  );
}
