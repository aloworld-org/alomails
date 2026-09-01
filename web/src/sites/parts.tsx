// The pieces the sites list pages and dialogs share, so the module reads as
// one surface rather than screens that drifted apart. Presentational only:
// no data loading, no rules. (Deliberately the module's own copies rather
// than imports from billing — the two modules belong to different tracks and
// must not couple; promoting this dialog chrome into `ds` is a wave-review
// candidate once three modules carry it.)
import { useRef, type FormEvent, type ReactNode } from "react";
import { Info, X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { strings } from "../i18n";
import { Button, MODAL_BACKDROP_CLASS } from "../ds";
import { useDialogKeyboard } from "./useDialogKeyboard";

/** A failure the page could not hide: shown, never swallowed. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p
      className="mx-6 my-3 rounded-lg border border-danger bg-danger-tint px-4 py-3 text-sm text-primary"
      role="alert"
    >
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
    <div className="flex min-h-80 flex-1 flex-col items-center justify-center gap-3 px-8 py-12 text-center">
      <span
        className="inline-flex size-20 items-center justify-center rounded-full bg-accent-soft text-accent"
        aria-hidden="true"
      >
        <Icon size={38} />
      </span>
      <h2 className="m-0 text-xl font-semibold text-primary">{title}</h2>
      <p className="mb-2 max-w-[42ch] text-base text-secondary">{body}</p>
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
  hintDisplay = "below",
  children,
}: {
  label: string;
  hint?: string | undefined;
  hintDisplay?: "below" | "tooltip" | undefined;
  children: ReactNode;
}) {
  if (hint !== undefined && hintDisplay === "tooltip") {
    return (
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center gap-1.5 text-sm font-semibold text-primary">
          <span>{label}</span>
          <InformationTip label={label} hint={hint} />
        </div>
        <label className="flex flex-col gap-1.5">
          <span className="sr-only">{label}</span>
          {children}
        </label>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-1.5">
      <label className="flex flex-col gap-1.5">
        <span className="text-sm font-semibold text-primary">{label}</span>
        {children}
      </label>
      {hint !== undefined && <span className="text-sm text-secondary">{hint}</span>}
    </div>
  );
}

/** Longer guidance that stays out of the form until somebody asks for it.
 *  Focus mirrors hover so the information is equally available by keyboard. */
export function InformationTip({
  label,
  hint,
}: {
  label: string;
  hint: string;
}) {
  return (
    <span
      className="group relative inline-flex size-6 shrink-0 cursor-help items-center justify-center rounded-full text-tertiary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:bg-accent-soft focus-visible:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/20"
      role="note"
      tabIndex={0}
      aria-label={`${label}: ${hint}`}
    >
      <Info className="size-4" aria-hidden="true" />
      <span
        className="pointer-events-none absolute left-0 top-[calc(100%+.4rem)] z-30 hidden w-max max-w-72 rounded-lg bg-primary px-3 py-2 text-left text-xs font-normal leading-relaxed text-on-accent shadow-lg group-hover:block group-focus-visible:block"
        role="tooltip"
      >
        {hint}
      </span>
    </span>
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
    <div
      className={`fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-scrim p-4 sm:p-6 ${MODAL_BACKDROP_CLASS}`}
      role="presentation"
      onMouseDown={onClose}
    >
      <form
        ref={panel}
        className={`flex max-h-[min(52rem,calc(100dvh-2rem))] w-full flex-col overflow-hidden rounded-2xl border border-default bg-surface shadow-xl sm:max-h-[min(52rem,calc(100dvh-3rem))] ${wide ? "max-w-5xl" : "max-w-xl"}`}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        onSubmit={submit}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-start gap-3 border-b border-subtle px-5 py-4 sm:px-6">
          <span
            className="inline-flex size-10 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent"
            aria-hidden="true"
          >
            <Icon size={19} />
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="m-0 text-lg font-semibold text-primary">{title}</h2>
            <p className="mt-0.5 text-sm leading-5 text-secondary">{subtitle}</p>
          </div>
          <button
            type="button"
            className="inline-flex size-10 shrink-0 items-center justify-center rounded-xl text-secondary transition-colors hover:bg-raised hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            onClick={onClose}
            // "Close", not "Cancel": the footer already carries a Cancel, and
            // two controls with one name in one dialog is a list of identical
            // choices to anybody reading it through the rotor. (S2.16b)
            aria-label={strings.close}
          >
            <X size={18} />
          </button>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-5 py-5 sm:px-6">
          {error !== null && <ErrorBanner message={error} />}
          {children}
        </div>
        <div className="flex shrink-0 items-center justify-end gap-3 border-t border-subtle bg-surface px-5 py-4 sm:px-6">
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
