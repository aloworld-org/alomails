// One invoice: the document's own words on the shared editor shell, plus the
// three things that can happen to an invoice and nothing else.
//
// - **Issuing** spends the next number of the tenant's gapless series and
//   freezes the document for ever, so it is offered behind a dialog that says
//   exactly that rather than "are you sure".
// - **Voiding** cancels a document nobody acted on; it keeps its number and
//   stays readable, because a number that vanished is a hole in the series.
// - **A credit note** is the correction for a document the customer already
//   holds: the server raises a draft mirroring every line, and this screen
//   goes straight to it, because it is the thing that now needs editing.
//
// None of the three is decided here. Each is a `POST` the store rules on under
// the document's row lock, and what comes back — the frozen document, or the
// new draft — is what the screen then shows.
import { useCallback } from "react";
import { useNavigate, useParams } from "react-router-dom";

import { strings, useLocale } from "../i18n";
import { useBillingApi } from "./api";
import { formatDocumentDate } from "./dates";
import type { DocumentAction } from "./DocumentActions";
import { DocumentEditor } from "./DocumentEditor";
import type { DocumentHeader, DocumentPatch } from "./documentDraft";
import { useDocumentDraft } from "./documentDraft";
import { Field } from "./parts";
import { usePickers } from "./pickers";
import { DocumentChips } from "./status";
import type { BillingInvoice, BillingInvoiceSummary } from "./types";
import styles from "./BillingModule.module.css";

export function InvoiceEditor() {
  const { id } = useParams<{ id: string }>();
  const api = useBillingApi();
  const locale = useLocale();
  const navigate = useNavigate();
  const pickers = usePickers();

  // The ports are memoized because the draft hook keys its load effect and its
  // autosave debounce on them: a fresh closure per render would restart the
  // timer on every keystroke, and the draft would never save itself.
  const load = useCallback(
    async (documentId: string) => {
      const loaded = await api.invoice(documentId);
      return { document: loaded.invoice, aside: loaded.creditNotes };
    },
    [api],
  );
  const create = useCallback(
    (header: Partial<DocumentHeader>) => api.createInvoice(header),
    [api],
  );
  const save = useCallback(
    (documentId: string, patch: DocumentPatch) => api.updateInvoice(documentId, patch),
    [api],
  );
  const editable = useCallback((invoice: BillingInvoice) => invoice.status === "draft", []);

  const draft = useDocumentDraft<BillingInvoice, BillingInvoiceSummary[]>({
    id,
    load,
    create,
    save,
    editable,
  });
  const invoice = draft.document;

  const actions: DocumentAction[] = [];
  if (invoice !== null && id !== undefined) {
    if (invoice.status === "draft") {
      actions.push({
        key: "issue",
        label: strings.billingIssue,
        title: strings.billingIssueTitle,
        message: strings.billingIssueConfirm,
        primary: true,
        run: async () => {
          draft.adopt(await api.issueInvoice(id));
        },
      });
    }
    // A document the customer may already hold is corrected, not cancelled —
    // so crediting leads and voiding is the quiet option beside it.
    if (invoice.status === "issued" || invoice.status === "paid") {
      actions.push({
        key: "credit-note",
        label: strings.billingCreditNoteAction,
        title: strings.billingCreditNoteTitle,
        message: strings.billingCreditNoteConfirm,
        primary: true,
        run: async () => {
          const credit = await api.createCreditNote(id);
          await navigate(`../${credit.id}`);
        },
      });
    }
    if (invoice.status === "issued") {
      actions.push({
        key: "void",
        label: strings.billingVoid,
        title: strings.billingVoidTitle,
        message: strings.billingVoidConfirm,
        danger: true,
        run: async () => {
          draft.adopt(await api.voidInvoice(id));
        },
      });
    }
  }

  const creditNotes = draft.aside ?? [];

  return (
    <DocumentEditor
      draft={draft}
      pickers={pickers}
      labels={{
        newTitle: strings.billingNewInvoice,
        draftTitle: strings.billingDraftInvoice,
        back: strings.billingBackToInvoices,
        gone: strings.billingInvoiceGone,
        customerHint: strings.billingCustomerFixedHint,
        createHint: strings.billingCreateDraftHint,
        createLabel: strings.billingCreateDraft,
        discardLabel: strings.billingDeleteDraft,
        discardMessage: strings.billingDeleteDraftConfirm,
        frozenNotice:
          invoice?.status === "void" ? strings.billingVoidNotice : strings.billingFrozenNotice,
      }}
      chips={invoice === null ? null : <DocumentChips invoice={invoice} />}
      dates={
        invoice === null ? null : (
          <>
            <Field label={strings.billingFieldIssueDate}>
              <p className={styles.readOnlyValue}>
                {formatDocumentDate(invoice.issueDate, locale, strings.billingNoDate)}
              </p>
            </Field>
            <Field
              label={strings.billingFieldDueDate}
              hint={strings.billingTermsDays(invoice.paymentTermsDays)}
            >
              <p className={styles.readOnlyValue}>
                {formatDocumentDate(invoice.dueDate, locale, strings.billingNoDate)}
              </p>
            </Field>
          </>
        )
      }
      actions={actions}
      footer={
        invoice === null ? null : (
          <>
            {invoice.quoteId !== null && (
              <p className={styles.relation}>
                <button
                  type="button"
                  className={styles.linkAction}
                  onClick={() => void navigate(`../../quotes/${invoice.quoteId ?? ""}`)}
                >
                  {strings.billingFromQuote}
                </button>
              </p>
            )}
            {invoice.creditsInvoiceId !== null && (
              <p className={styles.relation}>
                <button
                  type="button"
                  className={styles.linkAction}
                  onClick={() => void navigate(`../${invoice.creditsInvoiceId ?? ""}`)}
                >
                  {strings.billingCreditsInvoice}
                </button>
              </p>
            )}
            {creditNotes.length > 0 && (
              <section className={styles.lines}>
                <h2 className={styles.sectionTitle}>{strings.billingCreditNotes}</h2>
                <ul className={styles.creditList}>
                  {creditNotes.map((credit) => (
                    <li key={credit.id}>
                      <button
                        type="button"
                        className={`${styles.rowName} ${styles.mono}`}
                        onClick={() => void navigate(`../${credit.id}`)}
                      >
                        {credit.number ?? strings.billingDraftInvoice}
                      </button>
                      <DocumentChips invoice={credit} />
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </>
        )
      }
      onBack={() => void navigate("..")}
      onCreated={(created) => {
        // Replaces the /new entry, so Back goes to the list rather than to a
        // form for a document that now exists.
        void navigate(`../${created.id}`, { replace: true });
      }}
      onDiscard={async () => {
        if (id === undefined) return;
        await api.deleteInvoice(id);
        await navigate("..");
      }}
    />
  );
}
