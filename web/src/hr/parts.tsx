// The pieces the HR screens share, so the board, the drawer and the two forms
// are visibly one module. Presentational only: no data loading, no rules, no
// dates computed, no access decided.
import type { FormEvent, ReactNode } from "react";
import { X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import styles from "./hr.module.css";

/** A failure the page could not hide: shown, never swallowed. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p className={styles.error} role="alert">
      {message}
    </p>
  );
}

/** The first-run state of a screen, with the action that ends it when there is
 *  one. A hiring board with no roles on it has one; a closed round with nobody
 *  in a column does not, because there is nothing to add there. */
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

/** The tone of a small coloured word. */
export type ChipTone = "info" | "good" | "bad";

/** A small coloured word for a state the server decided. */
export function Chip({ tone, children }: { tone: ChipTone; children: ReactNode }) {
  const toneClass =
    tone === "good" ? styles.chipGood : tone === "bad" ? styles.chipBad : styles.chipInfo;
  return <span className={`${styles.chip} ${toneClass}`}>{children}</span>;
}

/** One labelled control in a form. `hint` explains a rule the server owns;
 *  `error` is what the edge could not turn into what the server takes. */
export function Field({
  label,
  hint,
  error,
  children,
}: {
  label: string;
  hint?: string;
  error?: string | undefined;
  children: ReactNode;
}) {
  return (
    <label className={styles.field}>
      <span className={styles.label}>{label}</span>
      {children}
      {error !== undefined && <span className={styles.fieldError}>{error}</span>}
      {error === undefined && hint !== undefined && <span className={styles.hint}>{hint}</span>}
    </label>
  );
}

/** The modal chrome the two HR forms sit in: header, scrolling body, and a
 *  footer whose primary action is the form's submit. */
export function DialogFrame({
  Icon,
  title,
  subtitle,
  error,
  busy,
  canSubmit,
  submitLabel,
  onClose,
  onSubmit,
  children,
}: {
  Icon: LucideIcon;
  title: string;
  subtitle: string;
  error: string | null;
  busy: boolean;
  canSubmit: boolean;
  submitLabel: string;
  onClose: () => void;
  onSubmit: () => void;
  children: ReactNode;
}) {
  function submit(e: FormEvent) {
    e.preventDefault();
    if (!busy && canSubmit) onSubmit();
  }
  return (
    <div className={styles.scrim} role="presentation" onMouseDown={onClose}>
      <form
        className={styles.modal}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onSubmit={submit}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      >
        <div className={styles.modalHead}>
          <span className={styles.modalIcon} aria-hidden="true">
            <Icon size={19} />
          </span>
          <div className={styles.modalHeadText}>
            <h2>{title}</h2>
            <p>{subtitle}</p>
          </div>
          <button
            type="button"
            className={styles.modalClose}
            onClick={onClose}
            aria-label={strings.hrCancel}
          >
            <X size={18} />
          </button>
        </div>
        <div className={styles.modalBody}>
          {error !== null && <ErrorBanner message={error} />}
          {children}
        </div>
        <div className={styles.modalFooter}>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.hrCancel}
          </Button>
          <Button type="submit" disabled={busy || !canSubmit}>
            {submitLabel}
          </Button>
        </div>
      </form>
    </div>
  );
}
