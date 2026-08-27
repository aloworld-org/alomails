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
import { useCallback, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Eye, Palette, Pencil } from "lucide-react";

import { RecordHistory } from "../audit";
import { strings, useLocale } from "../i18n";
import { useBillingApi } from "./api";
import { formatDocumentDate } from "./dates";
import type { DocumentAction } from "./DocumentActions";
import { DocumentEditor } from "./DocumentEditor";
import type { CreationTemplate } from "./DocumentEditor";
import type { DocumentHeader, DocumentPatch } from "./documentDraft";
import { useDocumentDraft } from "./documentDraft";
import { blankRow, rowFromProduct } from "./lineRows";
import { Field } from "./parts";
import { usePickers } from "./pickers";
import { QuoteChips } from "./status";
import {
  DEFAULT_QUOTE_COLUMNS,
  QuoteContentStudio,
  type QuoteColumns,
  type QuoteContentStudioHandle,
} from "./QuoteContentStudio";
import type { BillingQuote } from "./types";
import styles from "./billingStyles";

export function QuoteEditor() {
  const { id } = useParams<{ id: string }>();
  const api = useBillingApi();
  const locale = useLocale();
  const navigate = useNavigate();
  const pickers = usePickers();
  const quoteStudio = useRef<QuoteContentStudioHandle>(null);
  const [preview, setPreview] = useState(false);
  const [quoteColumns, setQuoteColumns] = useState<QuoteColumns>(
    DEFAULT_QUOTE_COLUMNS,
  );

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
  const create = useCallback(
    (header: Partial<DocumentHeader>) => api.createQuote(header),
    [api],
  );
  const save = useCallback(
    (documentId: string, patch: DocumentPatch) =>
      api.updateQuote(documentId, patch),
    [api],
  );
  const editable = useCallback(
    (quote: BillingQuote) => quote.status === "draft",
    [],
  );

  const draft = useDocumentDraft<BillingQuote, string | null>({
    id,
    load,
    create,
    save,
    editable,
  });
  const quote = draft.document;

  const editAsDraft = useCallback(async () => {
    if (quote === null) return;
    if (quote.status === "draft") {
      quoteStudio.current?.edit();
      return;
    }
    const revised = await api.createQuote({
      customerId: quote.customerId,
      currency: quote.currency,
      validDays: quote.validDays,
      reference: quote.reference,
      note: quote.note,
      lines: quote.lines.map((line) => ({
        description: line.description,
        unit: line.unit,
        qtyMilli: line.qtyMilli,
        unitPriceCents: line.unitPriceCents,
        vatRateBp: line.vatRateBp,
        ...(line.productId == null ? {} : { productId: line.productId }),
      })),
    });
    await quoteStudio.current?.copyTo(revised.id).catch(() => undefined);
    await navigate(`../${revised.id}`, { replace: false });
  }, [api, navigate, quote]);

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
            // What the offer became depends on its lines: one naming a
            // price-list item is for goods and raises a sales order, one naming
            // none raises a draft invoice. Follow whichever the server actually
            // made — assuming either is how a screen ends up navigating to a
            // document that was never created.
            // Truthiness rather than `!== null`, deliberately: an older server
            // omits the field entirely, and `undefined !== null` would have
            // navigated to `/sales-orders/undefined`. A test caught exactly
            // that.
            const accepted = await api.acceptQuote(id);
            if (accepted.salesOrder) {
              await navigate(
                `/inventory/sales-orders/${accepted.salesOrder.id}`,
              );
            } else if (accepted.invoice) {
              await openInvoice(accepted.invoice.id);
            }
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
  const services = pickers.products.filter(
    (product) => !product.stocked && !product.archived,
  );
  const rowsFromProducts =
    (products: typeof services) => (nextKey: () => string) =>
      products.map((product) =>
        rowFromProduct({ ...blankRow(nextKey()), qty: "1" }, product),
      );
  const monthly = services.find(
    (product) => product.unit.toLowerCase() === "month",
  );
  const creationTemplates: CreationTemplate[] = [
    {
      key: "blank",
      name: strings.billingQuoteTemplateBlank,
      description: strings.billingQuoteTemplateBlankDescription,
      buildRows: () => [],
    },
    {
      key: "services",
      name: strings.billingQuoteTemplateServices,
      description: strings.billingQuoteTemplateServicesDescription,
      buildRows: rowsFromProducts(services.slice(0, 2)),
    },
    {
      key: "project",
      name: strings.billingQuoteTemplateProject,
      description: strings.billingQuoteTemplateProjectDescription,
      buildRows: rowsFromProducts(services.slice(0, 3)),
    },
    {
      key: "retainer",
      name: strings.billingQuoteTemplateRetainer,
      description: strings.billingQuoteTemplateRetainerDescription,
      buildRows: rowsFromProducts(
        monthly === undefined ? services.slice(0, 1) : [monthly],
      ),
    },
  ];

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
        createLabel: strings.billingQuoteContinueToEditor,
        discardLabel: strings.billingDeleteQuoteDraft,
        discardMessage: strings.billingDeleteQuoteDraftConfirm,
        frozenNotice:
          quote?.status === "sent" ? null : strings.billingQuoteClosedNotice,
      }}
      chips={quote === null ? null : <QuoteChips quote={quote} />}
      showSummary={!draft.readOnly && !preview}
      lineColumns={quoteColumns}
      documentBody={
        id === undefined
          ? null
          : (pricingTable, _totals, lineKeys, tableSubtotal) => (
              <QuoteContentStudio
                ref={quoteStudio}
                quoteId={id}
                readOnly={draft.readOnly || preview}
                preview={preview}
                pricingTable={pricingTable}
                tableSubtotal={tableSubtotal}
                lineKeys={lineKeys}
                onColumnsChange={setQuoteColumns}
              />
            )
      }
      editorActions={
        quote === null ? null : (
          <div className="flex items-center gap-2">
            <button
              type="button"
              className={styles.linkAction}
              onClick={() => void editAsDraft()}
              disabled={preview}
              title={
                preview
                  ? "Exit preview before editing this quote"
                  : quote.status === "draft"
                  ? "Edit quotation content"
                  : "Create an editable revision"
              }
            >
              <Pencil size={15} aria-hidden="true" /> Edit quote
            </button>
            <button
              type="button"
              className={styles.linkAction}
              onClick={() => quoteStudio.current?.customize()}
              disabled={quote.status !== "draft" || preview}
              title={
                preview
                  ? "Exit preview before customizing this quote"
                  : quote.status !== "draft"
                    ? "Create an editable revision before customizing"
                    : "Customize quotation"
              }
            >
              <Palette size={15} aria-hidden="true" /> Customize
            </button>
            <button
              type="button"
              className={styles.linkAction}
              aria-pressed={preview}
              onClick={() => setPreview((value) => !value)}
            >
              <Eye size={15} aria-hidden="true" />
              {preview ? "Exit preview" : "Preview"}
            </button>
          </div>
        )
      }
      presentationReadOnly={preview}
      dates={
        quote === null ? null : (
          <>
            <Field label={strings.billingFieldSentDate}>
              <p className={styles.readOnlyValue}>
                {formatDocumentDate(
                  quote.sentDate,
                  locale,
                  strings.billingNoDate,
                )}
              </p>
            </Field>
            <Field
              label={strings.billingFieldValidUntil}
              hint={strings.billingValidForDays(quote.validDays)}
            >
              <p className={styles.readOnlyValue}>
                {formatDocumentDate(
                  quote.validUntil,
                  locale,
                  strings.billingNoDate,
                )}
              </p>
            </Field>
          </>
        )
      }
      actions={actions}
      creationTemplates={creationTemplates}
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
            {id !== undefined && (
              <RecordHistory
                entityType="billing.quote"
                entityId={id}
                note={
                  quote?.status === "sent"
                    ? strings.billingQuoteSentNotice
                    : undefined
                }
              />
            )}
          </>
        )
      }
      onBack={() => void navigate("..")}
      onCreated={(created) => {
        void navigate(`../${created.id}`, { replace: true });
      }}
      onPrint={
        id === undefined ? undefined : () => api.documentHtml("quotes", id)
      }
      onDiscard={async () => {
        if (id === undefined) return;
        await api.deleteQuote(id);
        await navigate("..");
      }}
    />
  );
}
