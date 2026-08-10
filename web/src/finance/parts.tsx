// The pieces the Finance screens share, so the claims list, the claim form and
// the approver's two queues are visibly one module — and visibly the same
// module family as Billing, CRM and Projects, whose parts these mirror.
// Presentational only: no data loading, no rules, no arithmetic.
import type { FormEvent, ReactNode } from "react";
import { X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { Button, cx } from "../ds";
import { strings } from "../i18n";
import { statusLabel, statusTone } from "./format";
import type { ExpenseStatus } from "./types";
import styles from "./FinanceModule.module.css";

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
 *  one. The approver's queues have none — offering "add a claim to approve"
 *  would invent an action nobody can take. */
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

/** The coloured word for where a claim stands, as the server decided it. */
export function StatusChip({ status }: { status: ExpenseStatus }) {
  return (
    <span className={cx(styles.chip, styles[`chip_${statusTone(status)}`])}>
      {statusLabel(status)}
    </span>
  );
}

/** One labelled control in a form. `hint` explains a rule the server owns;
 *  `error` is what this edge could not turn into what the server takes.
 *
 *  The hint and the error sit **outside** the `<label>` on purpose: a control's
 *  accessible name should be "VAT", not "VAT the VAT shown on the receipt,
 *  leave empty if it shows none" — a name is what the field is called, and a
 *  sentence read out on every focus is a sentence people learn to ignore. */
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
    <div className={styles.field}>
      <label className={styles.fieldLabel}>
        <span className={styles.label}>{label}</span>
        {children}
      </label>
      {error !== undefined && <span className={styles.fieldError}>{error}</span>}
      {error === undefined && hint !== undefined && <span className={styles.hint}>{hint}</span>}
    </div>
  );
}

/** The modal chrome a Finance form sits in: header, scrolling body, and a
 *  footer whose primary action is the form's submit. `extraAction` is the
 *  destructive-but-not-primary one (deleting a draft claim), kept at the far
 *  left so it is never the button under a thumb. */
export function DialogFrame({
  Icon,
  title,
  subtitle,
  error,
  busy,
  canSubmit,
  submitLabel,
  extraAction,
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
  extraAction?: { label: string; onClick: () => void } | undefined;
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
            aria-label={strings.financeCancel}
          >
            <X size={18} />
          </button>
        </div>
        <div className={styles.modalBody}>
          {error !== null && <ErrorBanner message={error} />}
          {children}
        </div>
        <div className={styles.modalFooter}>
          {extraAction !== undefined && (
            <Button variant="ghost" onClick={extraAction.onClick} disabled={busy}>
              {extraAction.label}
            </Button>
          )}
          <span className={styles.modalFooterSpacer} />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.financeCancel}
          </Button>
          <Button type="submit" disabled={busy || !canSubmit}>
            {submitLabel}
          </Button>
        </div>
      </form>
    </div>
  );
}
