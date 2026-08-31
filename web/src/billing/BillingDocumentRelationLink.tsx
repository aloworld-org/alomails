import { ArrowUpRight } from "lucide-react";

import styles from "./billingStyles";

type Props = {
  label: string;
  onOpen: () => void;
};

export function BillingDocumentRelationLink({ label, onOpen }: Props) {
  return (
    <button type="button" className={styles.linkAction} onClick={onOpen}>
      {label}
      <ArrowUpRight size={14} aria-hidden="true" />
    </button>
  );
}
