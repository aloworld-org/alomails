// The small pieces the Insights screens share, so the tab strip, the grid and
// the cards read as one module. Presentational only: no data loading, no rules,
// no arithmetic.
import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";

import { Button } from "../ds";
import styles from "./InsightsModule.module.css";

/** A failure the page could not hide: shown, never swallowed. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p className={styles.error} role="alert">
      {message}
    </p>
  );
}

/** The first-run state of a screen, with the action that ends it when there is
 *  one. A board a tenant can make has one; a board with nothing pinned to it
 *  does not — offering a button that leads nowhere would invent an action. */
export function EmptyState({
  Icon,
  title,
  body,
  cta,
  onCta,
}: {
  Icon: LucideIcon;
  title: string;
  body: string;
  cta?: string;
  onCta?: () => void;
}) {
  return (
    <div className={styles.empty}>
      <span className={styles.emptyArt} aria-hidden="true">
        <Icon size={38} />
      </span>
      <h2 className={styles.emptyTitle}>{title}</h2>
      <p className={styles.emptyBody}>{body}</p>
      {cta !== undefined && onCta !== undefined && <Button onClick={onCta}>{cta}</Button>}
    </div>
  );
}

/** A row of chrome above the grid — the board's own name and its actions. */
export function BoardBar({ children }: { children: ReactNode }) {
  return <div className={styles.boardBar}>{children}</div>;
}
