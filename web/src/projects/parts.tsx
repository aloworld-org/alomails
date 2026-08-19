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

/** A failure the page could not hide: shown, never swallowed. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p className="mx-5 my-3 rounded-md bg-[var(--danger-tint)] px-3.5 py-2.5 text-sm text-danger" role="alert">
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
    <div className="flex flex-col items-center gap-3 px-5 py-8 text-center text-secondary">
      <span className="text-tertiary" aria-hidden="true">
        <Icon size={38} />
      </span>
      <h2 className="m-0 text-lg font-semibold text-primary">{title}</h2>
      <p className="m-0 max-w-[46ch] text-sm">{body}</p>
      {cta !== undefined && onCta !== undefined && <Button onClick={onCta}>{cta}</Button>}
    </div>
  );
}

/** The coloured word for where a week stands, as the server decided it. */
export function WeekChip({ status }: { status: WeekStatus }) {
  const tone =
    status === "approved"
      ? "bg-[var(--success-tint)] text-success"
      : status === "rejected"
        ? "bg-[var(--danger-tint)] text-danger"
        : status === "submitted"
          ? "bg-[var(--navy-50)] text-[var(--navy-600)]"
          : "bg-raised text-secondary";
  return <span className={`inline-flex items-center whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-medium ${tone}`}>{weekStatusLabel(status)}</span>;
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
    <div className="flex min-w-32 items-center gap-2">
      <div
        className="h-1.5 flex-1 overflow-hidden rounded-full bg-raised"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(consumptionBp / 100)}
        aria-label={label}
      >
        <span
          className={`block h-full rounded-full ${over ? "bg-danger" : "bg-accent"}`}
          style={{ width: `${width}%` }}
        />
      </div>
      <span className={`text-xs tabular-nums ${over ? "font-medium text-danger" : "text-tertiary"}`}>
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
    <label className="flex flex-col gap-1.5">
      <span className="text-sm font-medium text-secondary">{label}</span>
      {children}
      {error !== undefined && <span className="text-xs text-danger">{error}</span>}
      {error === undefined && hint !== undefined && <span className="text-xs text-tertiary">{hint}</span>}
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
    <div className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-overlay p-4" role="presentation" onMouseDown={onClose}>
      <form
        className="flex max-h-[90vh] min-h-0 w-full max-w-[35rem] flex-col rounded-xl bg-surface shadow-lg"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onSubmit={submit}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      >
        <div className="flex items-start gap-3 border-b border-subtle px-5 py-4">
          <span className="inline-flex size-9 shrink-0 items-center justify-center rounded-md bg--soft text-accent" aria-hidden="true">
            <Icon size={19} />
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="m-0 text-lg font-semibold text-primary">{title}</h2>
            <p className="m-0 mt-0.5 text-sm text-tertiary">{subtitle}</p>
          </div>
          <button
            type="button"
            className="rounded-sm p-1 text-tertiary hover:bg-raised hover:text-primary"
            onClick={onClose}
            aria-label={strings.projectsCancel}
          >
            <X size={18} />
          </button>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-5">
          {error !== null && <ErrorBanner message={error} />}
          {children}
        </div>
        <div className="flex items-center gap-2 border-t border-subtle px-5 py-4">
          {extraAction !== undefined && (
            <Button variant="ghost" onClick={extraAction.onClick} disabled={busy}>
              {extraAction.label}
            </Button>
          )}
          <span className="flex-1" />
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
