// One quote, on the same shell as an invoice — because it is the same
// document until somebody says yes.
//
// The four transitions an offer has, and why each asks first:
//
// - **Send** assigns the offer's number and freezes its prices. What the
//   customer is holding cannot then quietly change under them.
// - **Accept** closes the offer *and* raises the draft invoice for it, in one
//   server transaction, with a copy of every line at the price it was offered
//   at. This screen then goes to that invoice, because that is the document
//   that now needs work — nothing has been issued yet.
// - **Decline** and **expire** close the offer without business. Both are
//   terminal: a change of mind is a new quote, not a reopened one.
//
// A lapsed offer can still be accepted. The store refuses on state, never on a
// date, and honouring an offer a few days late is a decision a tenant is
// entitled to make — so "Lapsed" is a chip here, not a locked door.
import { useCallback } from "react";
import { useNavigate, useParams } from "react-router-dom";

import { RecordHistory } from "../audit";
import { strings, useLocale } from "../i18n";
import { useBillingApi } from "./api";
import { formatDocumentDate } from "./dates";
import type { DocumentAction } from "./DocumentActions";
import { DocumentEditor } from "./DocumentEditor";
import type { DocumentHeader, DocumentPatch } from "./documentDraft";
import { useDocumentDraft } from "./documentDraft";
import { Field } from "./parts";
import { usePickers } from "./pickers";
import { QuoteChips } from "./status";
import type { BillingQuote } from "./types";
import styles from "./billingStyles";

export function QuoteEditor() {
  const { id } = useParams<{ id: string }>();
  const api = useBillingApi();
  const locale = useLocale();
  const navigate = useNavigate();
  const pickers = usePickers();

  /** The invoice screen for an id, from inside `/billing/quotes/{id}`. */
  const openInvoice = useCallback(
    async (invoiceId: string) => {
      await navigate(`../../invoices/${invoiceId}`);
    },
    [navigate],
  );

  // Memoized for the same reason as the invoice editor's: the draft hook keys
  // its load effect and its autosave debounce on them.
  const load = useCallback(
    async (documentId: string) => {
      const loaded = await api.quote(documentId);
      return { document: loaded.quote, aside: loaded.invoiceId };
    },
    [api],
  );
  const create = useCallback((header: Partial<DocumentHeader>) => api.createQuote(header), [api]);
  const save = useCallback(
    (documentId: string, patch: DocumentPatch) => api.updateQuote(documentId, patch),
    [api],
  );
  const editable = useCallback((quote: BillingQuote) => quote.status === "draft", []);

  const draft = useDocumentDraft<BillingQuote, string | null>({
    id,
    load,
    create,
    save,
    editable,
  });
  const quote = draft.document;

  const actions: DocumentAction[] = [];
  if (quote !== null && id !== undefined) {
    if (quote.status === "draft") {
      actions.push({
        key: "send",
        label: strings.billingSendQuote,
        title: strings.billingSendQuoteTitle,
        message: strings.billingSendQuoteConfirm,
        primary: true,
        run: async () => {
          draft.adopt(await api.sendQuote(id));
        },
      });
    }
    if (quote.status === "sent") {
      actions.push(
        {
          key: "accept",
          label: strings.billingAcceptQuote,
          title: strings.billingAcceptQuoteTitle,
          message: strings.billingAcceptQuoteConfirm,
          primary: true,
          run: async () => {
            const accepted = await api.acceptQuote(id);
            await openInvoice(accepted.invoice.id);
          },
        },
        {
          key: "decline",
          label: strings.billingDeclineQuote,
          title: strings.billingDeclineQuoteTitle,
          message: strings.billingDeclineQuoteConfirm,
          danger: true,
          run: async () => {
            draft.adopt(await api.declineQuote(id));
          },
        },
        {
          key: "expire",
          label: strings.billingExpireQuote,
          title: strings.billingExpireQuoteTitle,
          message: strings.billingExpireQuoteConfirm,
          run: async () => {
            draft.adopt(await api.expireQuote(id));
          },
        },
      );
    }
  }

  const invoiceId = draft.aside;

  return (
    <DocumentEditor
      draft={draft}
      pickers={pickers}
      labels={{
        newTitle: strings.billingNewQuote,
        draftTitle: strings.billingDraftQuote,
        back: strings.billingBackToQuotes,
        gone: strings.billingQuoteGone,
        customerHint: strings.billingQuoteCustomerHint,
        createHint: strings.billingCreateQuoteHint,
        createLabel: strings.billingCreateDraft,
        discardLabel: strings.billingDeleteQuoteDraft,
        discardMessage: strings.billingDeleteQuoteDraftConfirm,
        frozenNotice:
          quote?.status === "sent"
            ? strings.billingQuoteSentNotice
            : strings.billingQuoteClosedNotice,
      }}
      chips={quote === null ? null : <QuoteChips quote={quote} />}
      dates={
        quote === null ? null : (
          <>
            <Field label={strings.billingFieldSentDate}>
              <p className={styles.readOnlyValue}>
                {formatDocumentDate(quote.sentDate, locale, strings.billingNoDate)}
              </p>
            </Field>
            <Field
              label={strings.billingFieldValidUntil}
              hint={strings.billingValidForDays(quote.validDays)}
            >
              <p className={styles.readOnlyValue}>
                {formatDocumentDate(quote.validUntil, locale, strings.billingNoDate)}
              </p>
            </Field>
          </>
        )
      }
      actions={actions}
      footer={
        quote === null ? null : (
          <>
            {invoiceId !== null && (
              <p className={styles.relation}>
                <button
                  type="button"
                  className={styles.linkAction}
                  onClick={() => void openInvoice(invoiceId)}
                >
                  {strings.billingQuoteInvoice}
                </button>
              </p>
            )}
            {/* Who did what to this quote, and when (B2.13). A quote that was
                never saved has no id and therefore no history. */}
            {id !== undefined && <RecordHistory entityType="billing.quote" entityId={id} />}
          </>
        )
      }
      onBack={() => void navigate("..")}
      onCreated={(created) => {
        void navigate(`../${created.id}`, { replace: true });
      }}
      onPrint={id === undefined ? undefined : () => api.documentHtml("quotes", id)}
      onDiscard={async () => {
        if (id === undefined) return;
        await api.deleteQuote(id);
        await navigate("..");
      }}
    />
  );
}
