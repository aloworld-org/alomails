// The pieces the sites list pages and dialogs share, so the module reads as
// one surface rather than screens that drifted apart. Presentational only:
// no data loading, no rules. (Deliberately the module's own copies rather
// than imports from billing — the two modules belong to different tracks and
// must not couple; promoting this dialog chrome into `ds` is a wave-review
// candidate once three modules carry it.)
import { useRef, type FormEvent, type ReactNode } from "react";
import { X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { strings } from "../i18n";
import { Button } from "../ds";
import { useDialogKeyboard } from "./useDialogKeyboard";
import styles from "./SitesModule.module.css";

/** A failure the page could not hide: shown, never swallowed. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p className={styles.error} role="alert">
      {message}
    </p>
  );
}

/** The first-run state of a list, with the action that ends it. */
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

/** One labelled control in a dialog. `hint` explains a rule the server owns;
 *  it sits beside the label, not inside it, so the control's accessible name
 *  is the label alone. */
export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string | undefined;
  children: ReactNode;
}) {
  return (
    <div className={styles.field}>
      <label className={styles.fieldLabel}>
        <span className={styles.label}>{label}</span>
        {children}
      </label>
      {hint !== undefined && <span className={styles.hint}>{hint}</span>}
    </div>
  );
}

/** The modal chrome the sites forms sit in: header, scrolling body, and a
 *  footer whose primary action is the form's submit. */
export function DialogFrame({
  Icon,
  title,
  subtitle,
  error,
  busy,
  canSubmit,
  submitLabel,
  wide = false,
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
  /** Widens the modal for content a form column cannot hold — a gallery of
   *  cards with a rendered preview beside them. The narrow form stays the
   *  default, so nothing but the screen that asked for room gets it. */
  wide?: boolean;
  onClose: () => void;
  onSubmit: () => void;
  children: ReactNode;
}) {
  const panel = useRef<HTMLFormElement>(null);
  useDialogKeyboard(panel, onClose);
  function submit(e: FormEvent) {
    e.preventDefault();
    if (!busy && canSubmit) onSubmit();
  }
  return (
    <div className={styles.scrim} role="presentation" onMouseDown={onClose}>
      <form
        ref={panel}
        className={wide ? `${styles.modal} ${styles.modalWide}` : styles.modal}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        onSubmit={submit}
        onMouseDown={(e) => e.stopPropagation()}
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
            // "Close", not "Cancel": the footer already carries a Cancel, and
            // two controls with one name in one dialog is a list of identical
            // choices to anybody reading it through the rotor. (S2.16b)
            aria-label={strings.close}
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
            {strings.sitesCancel}
          </Button>
          <Button type="submit" disabled={busy || !canSubmit}>
            {submitLabel}
          </Button>
        </div>
      </form>
    </div>
  );
}
