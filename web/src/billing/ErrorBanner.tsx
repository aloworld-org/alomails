import styles from "./billingStyles";

export function ErrorBanner({ message }: { message: string }) {
  return <p className={styles.error} role="alert">{message}</p>;
}
