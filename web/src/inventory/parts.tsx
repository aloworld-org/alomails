// The pieces the Inventory screens share, so the catalog and the stock list
// read as one module — and as the same module family as Billing, CRM, Projects
// and Finance, whose parts these mirror. Presentational only: no data loading,
// no rules, no arithmetic.
//
// Since D2.09b the state word draws as a `ds/Badge`, and the module's own
// `Field` is gone entirely — `ds/Field` binds the label to the control and
// announces the error, which the hand-rolled column never did.
import type { LucideIcon } from "lucide-react";

import { Badge, Button } from "../ds";
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

/** The visual weight of a state chip. Named after what it means rather than
 *  after its colour, so a theme can restyle it without renaming anything.
 *  It stays the module's vocabulary rather than becoming the design system's
 *  tone names, because `format.ts` maps eleven order states onto it and that
 *  mapping is tested. */
export type ChipTone = "neutral" | "info" | "good" | "warn" | "muted";

/** A small state label: what an order is, or what is wrong with it.
 *
 *  A `ds/Badge` rather than a `ds/Chip`: the design system's line is that a
 *  badge is read and a chip is acted on, and not one of these is pressable.
 *  Only the drawing is the design system's — the five tones stay this
 *  module's, folded onto `Badge`'s four. `muted` loses its distinct quiet
 *  ink and reads as `neutral`, whose ink is already the tertiary colour. */
export function StatusChip({ tone, label }: { tone: ChipTone; label: string }) {
  return (
    <Badge
      tone={
        tone === "info"
          ? "accent"
          : tone === "good"
            ? "success"
            : tone === "warn"
              ? "danger"
              : "neutral"
      }
    >
      {label}
    </Badge>
  );
}
