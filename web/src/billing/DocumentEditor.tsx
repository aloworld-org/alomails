// One billing document on screen: the customer it is for, what is on it, what
// the server says it comes to, and what can be done to it next.
//
// It is the shell both the invoice editor and the quote editor render, because
// the two screens are the same screen with different words and different
// transitions. A quote a salesperson sends and the invoice it becomes must
// show the same lines, the same totals and the same "this is frozen now"
// story — mirroring them by construction is the only way that stays true.
//
// What varies is passed in: the words (`labels`), the two dates this kind of
// document carries, the state chips, the lifecycle actions, and whatever the
// record shows about its relations (`footer`). What does not vary lives here:
// the header fields, the line grid, the totals panel, the save indicator, the
// create bar, and the read-only rendering of a document that carries a number.
import type { ReactNode } from "react";
import { ArrowLeft } from "lucide-react";

import { Button, Spinner, cx, useDialogs } from "../ds";
import { strings } from "../i18n";
import { DocumentActions } from "./DocumentActions";
import type { DocumentAction } from "./DocumentActions";
import { DocumentLines } from "./DocumentLines";
import type { DocumentDraft, StoredDocument } from "./documentDraft";
import { ErrorBanner, Field } from "./parts";
import type { Pickers } from "./pickers";
import { TotalsPanel } from "./TotalsPanel";
import styles from "./BillingModule.module.css";

/** Everything this shell says out loud, so no English lives in it. */
export interface DocumentEditorLabels {
  /** The heading before the document exists ("New invoice"). */
  newTitle: string;
  /** The heading of a document that has no number yet ("Draft invoice"). */
  draftTitle: string;
  /** The link back to the list ("All invoices"). */
  back: string;
  /** Shown when the id in the URL names nothing. */
  gone: string;
  /** What picking a customer decides about the document. */
  customerHint: string;
  createHint: string;
  createLabel: string;
  /** Discarding a draft — the label, and what the dialog says first. */
  discardLabel: string;
  discardMessage: string;
  /** Why a numbered document offers no edits. */
  frozenNotice: string;
}

interface Props<T extends StoredDocument, A> {
  draft: DocumentDraft<T, A>;
  pickers: Pickers;
  labels: DocumentEditorLabels;
  /** The state chips of this document, or nothing before it exists. */
  chips: ReactNode;
  /** The two dates this kind of document carries, as `Field`s. */
  dates: ReactNode;
  /** What can be done to the document from here. Empty on a closed one. */
  actions: DocumentAction[];
  /** What the record shows about its relations — the credit notes against an
   *  invoice, the invoice an accepted offer produced. */
  footer?: ReactNode;
  onBack: () => void;
  /** Called with the document the server just created, to navigate to it. */
  onCreated: (document: T) => void;
  /** Discards the draft. Only ever offered while the document is editable. */
  onDiscard: () => Promise<void>;
}

export function DocumentEditor<T extends StoredDocument, A>({
  draft,
  pickers,
  labels,
  chips,
  dates,
  actions,
  footer,
  onBack,
  onCreated,
  onDiscard,
}: Props<T, A>) {
  const { confirm } = useDialogs();
  const { document, header, rows, readOnly } = draft;

  // An archived customer is still offered on the document already raised for
  // them — otherwise the picker would silently show no one and the next edit
  // would change who is being billed.
  const pickable = pickers.customers.filter((c) => !c.archived || c.id === header.customerId);
  const customerName = pickers.customers.find((c) => c.id === header.customerId)?.name ?? "";

  if (draft.loading) {
    return (
      <div className={styles.page}>
        <div className={styles.loading}>
          <Spinner size={20} />
        </div>
      </div>
    );
  }

  if (draft.missing) {
    return (
      <div className={styles.page}>
        <ErrorBanner message={labels.gone} />
        <p className={styles.noMatches}>
          <button type="button" className={styles.linkAction} onClick={onBack}>
            {labels.back}
          </button>
        </p>
      </div>
    );
  }

  async function create() {
    const created = await draft.create();
    if (created !== null) onCreated(created);
  }

  async function discard() {
    if (
      !(await confirm({
        title: labels.discardLabel,
        message: labels.discardMessage,
        confirmLabel: labels.discardLabel,
        danger: true,
      }))
    ) {
      return;
    }
    try {
      await onDiscard();
    } catch {
      draft.fail(strings.billingSaveFailed);
    }
  }

  const currency = document?.currency ?? "";
  const saved = draft.saveState === "saved";
  const error = draft.error ?? pickers.error;

  return (
    <div className={cx(styles.page, styles.editor)}>
      <div className={styles.editorHead}>
        <button type="button" className={styles.linkAction} onClick={onBack}>
          <ArrowLeft size={14} aria-hidden="true" /> {labels.back}
        </button>
        <h2 className={styles.editorTitle}>
          {document === null ? labels.newTitle : (document.number ?? labels.draftTitle)}
        </h2>
        <span className={styles.chips}>{chips}</span>
        <span className={styles.saveState} role="status">
          {document === null
            ? ""
            : draft.saveState === "saving"
              ? strings.billingSaving
              : draft.saveState === "pending"
                ? strings.billingUnsaved
                : draft.saveState === "failed"
                  ? strings.billingSaveNotDone
                  : strings.billingSaved}
        </span>
        {draft.saveState === "failed" && (
          <button type="button" className={styles.linkAction} onClick={draft.saveNow}>
            {strings.billingSaveNow}
          </button>
        )}
        {document !== null && !readOnly && (
          <button type="button" className={styles.linkAction} onClick={() => void discard()}>
            {labels.discardLabel}
          </button>
        )}
      </div>

      {error !== null && <ErrorBanner message={error} />}
      {readOnly && <p className={styles.notice}>{labels.frozenNotice}</p>}

      <div className={styles.editorBody}>
        <div className={styles.headerFields}>
          <Field label={strings.billingFieldCustomer} hint={labels.customerHint}>
            {readOnly ? (
              <p className={styles.readOnlyValue}>{customerName}</p>
            ) : (
              <select
                className={styles.input}
                value={header.customerId}
                onChange={(e) => draft.edit({ header: { ...header, customerId: e.target.value } })}
                aria-label={strings.billingFieldCustomer}
              >
                <option value="">{strings.billingChooseCustomer}</option>
                {pickable.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            )}
          </Field>

          <Field label={strings.billingFieldReference} hint={strings.billingReferenceHint}>
            {readOnly ? (
              <p className={styles.readOnlyValue}>{header.reference}</p>
            ) : (
              <input
                className={styles.input}
                value={header.reference}
                onChange={(e) => draft.edit({ header: { ...header, reference: e.target.value } })}
                placeholder={strings.billingReferencePlaceholder}
              />
            )}
          </Field>

          {document !== null && dates}
        </div>

        <Field label={strings.billingFieldNote} hint={strings.billingNoteHint}>
          {readOnly ? (
            <p className={styles.readOnlyValue}>{header.note}</p>
          ) : (
            <textarea
              className={cx(styles.input, styles.textarea)}
              value={header.note}
              rows={2}
              onChange={(e) => draft.edit({ header: { ...header, note: e.target.value } })}
              placeholder={strings.billingNotePlaceholder}
            />
          )}
        </Field>

        {document === null ? (
          <div className={styles.createBar}>
            <p className={styles.hint}>{labels.createHint}</p>
            <Button onClick={() => void create()} disabled={draft.creating || header.customerId === ""}>
              {labels.createLabel}
            </Button>
          </div>
        ) : (
          <>
            <DocumentLines
              rows={rows}
              products={pickers.products}
              savedLines={document.lines}
              saved={saved}
              currency={currency}
              readOnly={readOnly}
              onChange={(next) => draft.edit({ rows: next })}
              nextKey={draft.nextKey}
            />
            <TotalsPanel totals={document.totals} currency={currency} stale={!saved} />
            <DocumentActions actions={actions} unsaved={!saved} onFailed={draft.fail} />
            {footer}
          </>
        )}
      </div>
    </div>
  );
}
