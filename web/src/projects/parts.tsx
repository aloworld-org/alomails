// The pieces the Projects screens share, so the engagement list, the week grid,
// the approvals inbox and the engagement form are visibly one module — and
// visibly the same module family as Billing and CRM, whose parts these mirror.
// Presentational only: no data loading, no rules, no arithmetic.
import type { FormEvent, ReactNode } from "react";
import { X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { percentLabel, weekStatusLabel } from "./format";
import type { WeekStatus } from "./types";
import styles from "./ProjectsModule.module.css";

/** A failure the page could not hide: shown, never swallowed. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p className={styles.error} role="alert">
      {message}
    </p>
  );
}

/** The first-run state of a screen, with the action that ends it when there is
 *  one. A week with no hours in it has none — offering a button to "add a week"
 *  would invent an action. */
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

/** The coloured word for where a week stands, as the server decided it. */
export function WeekChip({ status }: { status: WeekStatus }) {
  const tone =
    status === "approved"
      ? styles.chipGood
      : status === "rejected"
        ? styles.chipBad
        : status === "submitted"
          ? styles.chipInfo
          : styles.chipQuiet;
  return <span className={`${styles.chip} ${tone}`}>{weekStatusLabel(status)}</span>;
}

/**
 * How much of an engagement's hours budget has been worked.
 *
 * `consumptionBp` is the **server's** figure in basis points; this draws it and
 * says it in words. Past the budget the bar fills and turns, because an overrun
 * is the one thing somebody opens this screen to find — clamping it at a full
 * bar would hide exactly that (`docs/design/projects.md` § Budgets: the budget
 * is advisory, and nothing here blocks).
 *
 * With no budget there is no proportion, and the bar is not drawn at all rather
 * than drawn empty — an empty bar reads as "none of it used".
 */
export function BudgetBar({
  consumptionBp,
  label,
}: {
  consumptionBp: number | null;
  label: string;
}) {
  if (consumptionBp === null) return null;
  const over = consumptionBp > 10_000;
  const width = Math.min(100, Math.max(0, consumptionBp / 100));
  return (
    <div className={styles.budget}>
      <div
        className={styles.budgetTrack}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(consumptionBp / 100)}
        aria-label={label}
      >
        <span
          className={`${styles.budgetFill} ${over ? styles.budgetOver : ""}`}
          style={{ width: `${width}%` }}
        />
      </div>
      <span className={over ? styles.budgetTextOver : styles.budgetText}>
        {percentLabel(consumptionBp)}
      </span>
    </div>
  );
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

/** The modal chrome a Projects form sits in: header, scrolling body, and a
 *  footer whose primary action is the form's submit. `extraAction` is the
 *  destructive-but-not-primary one (detaching an engagement, deleting an
 *  entry), kept at the far left so it is never the button under a thumb. */
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
            aria-label={strings.projectsCancel}
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
            {strings.projectsCancel}
          </Button>
          <Button type="submit" disabled={busy || !canSubmit}>
            {submitLabel}
          </Button>
        </div>
      </form>
    </div>
  );
}
