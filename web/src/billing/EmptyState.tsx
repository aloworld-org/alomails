import type { LucideIcon } from "lucide-react";
import { Button } from "../ds";
import styles from "./billingStyles";

export function EmptyState({ Icon, title, body, cta, onCta }: { Icon: LucideIcon; title: string; body: string; cta?: string; onCta?: () => void }) {
  return <div className={cta !== undefined ? `${styles.empty} ${styles.emptyWithAction}` : styles.empty}>
    <span className={styles.emptyArt} aria-hidden="true"><Icon size={38} /></span><h2 className={styles.emptyTitle}>{title}</h2><p className={styles.emptyBody}>{body}</p>
    {cta !== undefined && onCta !== undefined && <Button onClick={onCta}>{cta}</Button>}
  </div>;
}
