import type { ReactNode } from "react";

import styles from "./billingStyles";

/** Keeps table borders on a real cell while badges wrap inside it. */
export function BillingStatusCell({ children }: { children: ReactNode }) {
  return (
    <td>
      <span className={styles.chips}>{children}</span>
    </td>
  );
}
