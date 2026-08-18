// The pieces the two billing list pages and the two billing dialogs share, so
// customers and the price list are visibly one module rather than two screens
// that drifted apart. Presentational only: no data loading, no rules.
import { cloneElement, isValidElement, type FormEvent, type ReactNode } from "react";
import { Info, Plus, Search, X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import styles from "./billingStyles";

/** The bar above a list: search, the archived toggle, and the create action. */
export function Toolbar({
  search,
  onSearch,
  searchLabel,
  includeArchived,
  onIncludeArchived,
  createLabel,
  onCreate,
  busy,
  showCreate = true,
}: {
  search: string;
  onSearch: (v: string) => void;
  searchLabel: string;
  includeArchived: boolean;
  onIncludeArchived: (v: boolean) => void;
  createLabel: string;
  onCreate: () => void;
  busy: boolean;
  showCreate?: boolean;
}) {
  return (
    <div className={styles.toolbar}>
      <label className={styles.searchWrap}>
        <input className={styles.search} type="search" value={search} onChange={(e) => onSearch(e.target.value)} placeholder={searchLabel} aria-label={searchLabel} />
        <Search aria-hidden="true" />
      </label>
      <label className={styles.toggle}>
        <input
          type="checkbox"
          checked={includeArchived}
          onChange={(e) => onIncludeArchived(e.target.checked)}
        />
        {strings.billingShowArchived}
      </label>
      {busy && <Spinner size={16} />}
      {showCreate && <Button icon={<Plus aria-hidden="true" />} onClick={onCreate}>{createLabel}</Button>}
    </div>
  );
}

/** A failure the page could not hide: shown, never swallowed. */
export function ErrorBanner({ message }: { message: string }) {
  return (
    <p className={styles.error} role="alert">
      {message}
    </p>
  );
}

/** A stable first-load surface. It prevents an empty data table from flashing
 * before the backend has answered, without pretending that records exist. */
export function BillingLoading() {
  return (
    <div className={styles.dataLoading} role="status" aria-label={strings.billingLoading}>
      <Spinner size={24} />
      <span>{strings.billingLoading}</span>
    </div>
  );
}

/**
 * The first-run state of a list, with the action that ends it.
 *
 * The action is optional, because not every empty screen has one: a list a
 * user fills is empty until they create something, but a *report* over those
 * records is empty because the period holds nothing — and offering a button
 * there would invent an action that does not exist.
 */
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
    <div className={cta !== undefined ? `${styles.empty} ${styles.emptyWithAction}` : styles.empty}>
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
 *  `error` is what it answered when the rule was broken. */
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
  const control = isValidElement<{ "aria-label"?: string }>(children)
    ? cloneElement(children, { "aria-label": children.props["aria-label"] ?? label })
    : children;

  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <div className="flex min-w-0 items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-tertiary">
        <span>{label}</span>
        {hint !== undefined && error === undefined && (
          <button
            type="button"
            className="group relative inline-flex size-5 shrink-0 cursor-help items-center justify-center rounded-full text-tertiary outline-none transition-colors hover:bg-[var(--accent-soft)] hover:text-accent focus-visible:bg-[var(--accent-soft)] focus-visible:text-accent focus-visible:ring-2 focus-visible:ring-accent/20"
            aria-label={strings.sheetFormulaInformation}
          >
            <Info className="size-3.5" aria-hidden="true" />
            <span
              className="pointer-events-none absolute left-0 top-[calc(100%+.4rem)] z-20 hidden w-max max-w-72 rounded-lg bg-primary px-3 py-2 text-left text-xs font-normal normal-case leading-relaxed tracking-normal text-on-accent shadow-lg group-hover:block group-focus-visible:block"
              role="tooltip"
            >
              {hint}
            </span>
          </button>
        )}
      </div>
      {control}
      {error !== undefined && <span className="text-xs leading-relaxed text-danger">{error}</span>}
    </div>
  );
}

/** The modal chrome both billing forms sit in: header, scrolling body, and a
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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-overlay p-6 max-sm:p-3" role="presentation" onMouseDown={onClose}>
      <form
        className="flex max-h-[calc(100dvh-3rem)] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-subtle bg-surface shadow-lg max-sm:max-h-[calc(100dvh-1.5rem)]"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onSubmit={submit}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      >
        <div className="flex shrink-0 items-start gap-3 border-b border-subtle px-5 py-4">
          <span className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-[var(--accent-soft)] text-accent" aria-hidden="true">
            <Icon size={19} />
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="m-0 text-lg font-semibold text-primary">{title}</h2>
            <p className="mt-1 text-sm leading-relaxed text-secondary">{subtitle}</p>
          </div>
          <button
            type="button"
            className="flex size-9 shrink-0 items-center justify-center rounded-lg text-tertiary transition-colors hover:bg-raised hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            onClick={onClose}
            aria-label={strings.billingCancel}
          >
            <X size={18} />
          </button>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-5">
          {error !== null && <ErrorBanner message={error} />}
          {children}
        </div>
        <div className="flex shrink-0 justify-end gap-2 border-t border-subtle bg-surface px-5 py-4">
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.billingCancel}
          </Button>
          <Button type="submit" disabled={busy || !canSubmit}>
            {submitLabel}
          </Button>
        </div>
      </form>
    </div>
  );
}
