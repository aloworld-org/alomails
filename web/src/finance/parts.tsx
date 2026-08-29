// The pieces the Finance screens share, so the claims list, the claim form and
// the approver's two queues are visibly one module — and visibly the same
// module family as Billing, CRM and Projects, whose parts these mirror.
// Presentational only: no data loading, no rules, no arithmetic.
//
// Since D2.07b the primitives underneath are the design system's: a status word
// is a `ds/Badge`, and the dialog frame is a `ds/Modal` rather than this
// module's own scrim and panel. The module's own `Field` is gone entirely —
// `ds/Field` binds the label to the control by id and gives the error
// `role="alert"`, which is the part a wrapping `<label>` never had.
import { useId, type FormEvent, type ReactNode } from "react";
import { X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { Badge, Button, IconButton, Modal } from "../ds";
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
      {cta !== undefined && onCta !== undefined && (
        <Button onClick={onCta}>{cta}</Button>
      )}
    </div>
  );
}

/**
 * How loudly this module's two status words read, in `ds/Badge`'s named tones.
 *
 * `format.ts` decides the loudness — `statusTone` for a claim, `lineStatusTone`
 * for a staged bank line — and this is the one place that turns it into a
 * colour, so the two never drift apart. It is a badge and not a chip by the
 * design system's own line: **a badge is read, a chip is acted on**, and
 * nothing about "Approved" or "Set aside" is pressable.
 *
 * The tone is never the only signal: the word itself says which state it is.
 */
export const BADGE_TONE = {
  info: "accent",
  good: "success",
  bad: "danger",
  quiet: "neutral",
} as const;

/** The coloured word for where a claim stands, as the server decided it. */
export function StatusChip({ status }: { status: ExpenseStatus }) {
  return (
    <Badge tone={BADGE_TONE[statusTone(status)]}>{statusLabel(status)}</Badge>
  );
}

/** The modal chrome a Finance form sits in: a `ds/Modal` whose body is the form
 *  and whose footer submits it.
 *
 *  Body and footer are siblings inside the panel, so the submit button is tied
 *  to the form by id rather than nested in it — which is also what keeps Enter
 *  in any field working.
 *
 *  `extraAction` is the destructive-but-not-primary one (deleting a draft
 *  claim, deleting an unused account), kept at the far left so it is never the
 *  button under a thumb.
 *
 *  What the hand-built frame this replaces did not have: a focus trap (Tab
 *  walked straight out onto the screen behind), focus restored to whatever
 *  opened it, and an Escape that works wherever focus is — its `onKeyDown` sat
 *  on the `<form>`, so the one case it could not cover was the one the missing
 *  trap created. */
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
  aside,
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
  /** Rendered after the form, as its sibling — the slot for a panel that
   *  carries its own `<form>` (the record's agent), which HTML forbids
   *  nesting inside this frame's. */
  aside?: ReactNode;
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
          label={strings.financeCancel}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          {extraAction !== undefined && (
            <Button
              variant="ghost"
              onClick={extraAction.onClick}
              disabled={busy}
            >
              {extraAction.label}
            </Button>
          )}
          <div className="flex-1" />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.financeCancel}
          </Button>
          <Button type="submit" form={formId} disabled={busy || !canSubmit}>
            {submitLabel}
          </Button>
        </>
      }
    >
      {/* The sentence under the title. `ds/Modal`'s header is the name and the
          controls, so the question this dialog asks reads as the first line of
          the body rather than as a second heading. */}
      <p className="m-0 text-sm text-tertiary">{subtitle}</p>
      {error !== null && <ErrorBanner message={error} />}
      <form id={formId} className="flex flex-col gap-4" onSubmit={submit}>
        {children}
      </form>
      {aside}
    </Modal>
  );
}
