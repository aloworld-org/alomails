// The pieces the Projects screens share, so the engagement list, the week grid,
// the approvals inbox and the engagement form are visibly one module — and
// visibly the same module family as Billing and CRM, whose parts these mirror.
// Presentational only: no data loading, no rules, no arithmetic.
//
// Since D2.10b the primitives underneath are the design system's: the dialog
// frame is a `ds/Modal` rather than this module's own scrim and panel, and the
// state word draws as a `ds/Badge`. The module's own `Field` is gone entirely —
// `ds/Field` binds the label to the control and announces the error, which is
// the part the hand-rolled column never had.
import { useId, type FormEvent, type ReactNode } from "react";
import { Plus, X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { Badge, Button, IconButton, Modal } from "../ds";
import { strings } from "../i18n";
import { percentLabel, weekStatusLabel } from "./format";
import type { WeekStatus } from "./types";

/** A failure the page could not hide: shown, never swallowed. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p className="mx-5 my-3 rounded-md bg-danger-tint px-3.5 py-2.5 text-sm text-danger" role="alert">
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
    <section className="flex min-h-[26rem] flex-col items-center justify-center rounded-2xl border border-default bg-surface px-6 py-12 text-center shadow-sm max-sm:min-h-[22rem] max-sm:px-5">
      <span
        className="flex size-20 items-center justify-center rounded-full bg-accent-tint text-accent"
        aria-hidden="true"
      >
        <Icon size={36} strokeWidth={1.8} />
      </span>
      <h2 className="m-0 mt-5 text-xl font-bold tracking-tight text-primary">{title}</h2>
      <p className="m-0 mt-2 max-w-[42ch] text-base leading-7 text-secondary">{body}</p>
      {cta !== undefined && onCta !== undefined && (
        <Button className="mt-6" icon={<Plus aria-hidden="true" />} onClick={onCta}>
          {cta}
        </Button>
      )}
    </section>
  );
}

/** The coloured word for where a week stands, as the server decided it.
 *
 *  A `ds/Badge`: only the drawing is the design system's — the four week
 *  statuses stay this module's vocabulary, folded onto `Badge`'s tones.
 *  `submitted` reads as the accent rather than its former navy, which is the
 *  same fold inventory's order states made. */
export function WeekChip({ status }: { status: WeekStatus }) {
  const tone =
    status === "approved"
      ? "success"
      : status === "rejected"
        ? "danger"
        : status === "submitted"
          ? "accent"
          : "neutral";
  return <Badge tone={tone}>{weekStatusLabel(status)}</Badge>;
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

/** The modal chrome a Projects form sits in: a `ds/Modal` whose body is the
 *  form and whose footer submits it. Body and footer are siblings inside the
 *  panel, so the submit button is tied to the form by id rather than nested in
 *  it — which is also what keeps Enter in any field working.
 *
 *  What the panel this replaced never had: a **focus trap** — Tab walked
 *  straight out of all six dialogs onto the page behind them — and focus given
 *  back to the opener on close.
 *
 *  `extraAction` is the destructive-but-not-primary action (detaching an
 *  engagement, deleting an entry), kept at the far left so it is never the
 *  button under a thumb. */
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
  const formId = useId();
  function submit(e: FormEvent) {
    e.preventDefault();
    if (!busy && canSubmit) onSubmit();
  }
  return (
    <Modal
      title={title}
      onClose={onClose}
      icon={<Icon size={19} />}
      actions={
        <IconButton
          label={strings.projectsCancel}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          {extraAction !== undefined && (
            <Button variant="ghost" onClick={extraAction.onClick} disabled={busy}>
              {extraAction.label}
            </Button>
          )}
          <span className="flex-1" />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.projectsCancel}
          </Button>
          <Button type="submit" form={formId} disabled={busy || !canSubmit}>
            {submitLabel}
          </Button>
        </>
      }
    >
      {/* The sentence under the title. `ds/Modal`'s header is the name and the
          controls, so the question this dialog is asking reads as the first
          line of the body rather than as a second heading. */}
      <p className="m-0 text-sm text-tertiary">{subtitle}</p>
      {error !== null && <ErrorBanner message={error} />}
      <form id={formId} className="flex flex-col gap-4" onSubmit={submit}>
        {children}
      </form>
    </Modal>
  );
}
