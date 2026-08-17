// The pieces the two billing list pages and the two billing dialogs share, so
// customers and the price list are visibly one module rather than two screens
// that drifted apart. Presentational only: no data loading, no rules.
import type { FormEvent, ReactNode } from "react";
import { Plus, Search, X } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import styles from "./BillingModule.module.css";

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
  return (
    <label className={styles.field}>
      <span className={styles.label}>{label}</span>
      {children}
      {error !== undefined && <span className={styles.fieldError}>{error}</span>}
      {error === undefined && hint !== undefined && <span className={styles.hint}>{hint}</span>}
    </label>
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
            aria-label={strings.billingCancel}
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
