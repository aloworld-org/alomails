// The pieces the Inventory screens share, so the catalog and the stock list
// read as one module — and as the same module family as Billing, CRM, Projects
// and Finance, whose parts these mirror. Presentational only: no data loading,
// no rules, no arithmetic.
import type { LucideIcon } from "lucide-react";

import { Button } from "../ds";
import styles from "./InventoryModule.module.css";

/** A failure the page could not hide: shown, never swallowed, in the server's
 *  own words. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p className={styles.error} role="alert">
      {message}
    </p>
  );
}

/** The first-run state of a screen, with the action that ends it when there is
 *  one. A stock list has none — "add some stock" is not an act, a receipt or an
 *  adjustment is, and inventing a button here would point at a door this screen
 *  does not own. */
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
