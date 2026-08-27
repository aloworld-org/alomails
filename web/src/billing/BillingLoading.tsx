import { Spinner } from "../ds";
import { strings } from "../i18n";
import styles from "./billingStyles";

export function BillingLoading() {
  return <div className={styles.dataLoading} role="status" aria-label={strings.billingLoading}><Spinner size={24} /><span>{strings.billingLoading}</span></div>;
}
