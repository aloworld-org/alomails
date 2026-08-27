import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";

import { RecordHistory } from "../audit";
import { useDialogs } from "../ds";
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
import {
  DEFAULT_QUOTE_COLUMNS,
  QuoteContentStudio,
  saveQuoteTemplateDesign,
  type QuoteColumns,
  type QuoteContentStudioHandle,
} from "./QuoteContentStudio";
import type { BillingQuote, BillingSettings } from "./types";
import styles from "./billingStyles";
import { quoteCreationTemplates } from "./quoteCreationTemplates";
import { QuoteEditorToolbar } from "./QuoteEditorToolbar";

export function QuoteEditor() {
  const { id } = useParams<{ id: string }>();
  const api = useBillingApi();
  const locale = useLocale();
  const navigate = useNavigate();
  const pickers = usePickers();
  const { confirm } = useDialogs();
  const quoteStudio = useRef<QuoteContentStudioHandle>(null);
  const [searchParams, setSearchParams] = useSearchParams();
  const preview = searchParams.get("preview") === "1";
  const [issuer, setIssuer] = useState<BillingSettings | null>(null);
  const [quoteColumns, setQuoteColumns] = useState<QuoteColumns>(
    DEFAULT_QUOTE_COLUMNS,
  );
  const [creatingRevision, setCreatingRevision] = useState(false);

  useEffect(() => {
    let active = true;
    void api
      .settings()
      .then((settings) => {
        if (active && settings) setIssuer(settings);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [api]);

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

  const leavePreview = useCallback(
    (nextAction?: "edit" | "customize") => {
      const next = new URLSearchParams(searchParams);
      next.delete("preview");
      if (nextAction !== undefined) next.set("action", nextAction);
      setSearchParams(next, { replace: true });
    },
    [searchParams, setSearchParams],
  );

  useEffect(() => {
    if (quote?.status !== "draft" || preview) return;
    const action = searchParams.get("action");
    if (action !== "edit" && action !== "customize") return;
    const next = new URLSearchParams(searchParams);
    next.delete("action");
    setSearchParams(next, { replace: true });
    if (action === "customize") quoteStudio.current?.customize();
    else quoteStudio.current?.edit();
  }, [preview, quote, searchParams, setSearchParams]);

  const createRevision = useCallback(
    async (nextAction: "edit" | "customize") => {
      if (quote === null || creatingRevision) return;
      const accepted = await confirm({
        title: strings.billingQuoteCreateRevisionTitle,
        message: strings.billingQuoteCreateRevisionConfirm,
        confirmLabel: strings.billingQuoteCreateRevisionAction,
      });
      if (!accepted) return;
      setCreatingRevision(true);
      try {
        const created = await api.createQuote({
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
        await quoteStudio.current?.copyTo(created.id).catch(() => undefined);
        await navigate(`../${created.id}?action=${nextAction}`);
      } finally {
        setCreatingRevision(false);
      }
    },
    [api, confirm, creatingRevision, navigate, quote],
  );

  const editDraft = useCallback(() => {
    if (quote === null) return;
    if (quote.status !== "draft") {
      void createRevision("edit");
      return;
    }
    if (preview) {
      leavePreview("edit");
      return;
    }
    quoteStudio.current?.edit();
  }, [createRevision, leavePreview, preview, quote]);

  const customizeQuote = useCallback(() => {
    if (quote === null) return;
    if (quote.status !== "draft") {
      void createRevision("customize");
      return;
    }
    if (preview) {
      leavePreview("customize");
      return;
    }
    quoteStudio.current?.customize();
  }, [createRevision, leavePreview, preview, quote]);

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
  const creationTemplates = quoteCreationTemplates(pickers.products);

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
                issuer={issuer}
                quote={quote}
                customer={
                  pickers.customers.find(
                    (customer) => customer.id === quote?.customerId,
                  ) ?? null
                }
                customerName={
                  pickers.customers.find(
                    (customer) => customer.id === quote?.customerId,
                  )?.name ?? ""
                }
              />
            )
      }
      editorActions={
        quote === null ? null : (
          <QuoteEditorToolbar
            creatingRevision={creatingRevision}
            draft={quote.status === "draft"}
            preview={preview}
            onCustomize={customizeQuote}
            onEdit={editDraft}
            onTogglePreview={() => {
              const next = new URLSearchParams(searchParams);
              if (preview) next.delete("preview");
              else next.set("preview", "1");
              setSearchParams(next, { replace: true });
            }}
          />
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
      onCreated={(created, creationTemplate) => {
        const preset =
          creationTemplate === "services" ||
          creationTemplate === "project" ||
          creationTemplate === "retainer"
            ? creationTemplate
            : "blank";
        void saveQuoteTemplateDesign(created.id, preset)
          .catch(() => undefined)
          .finally(() => void navigate(`../${created.id}`, { replace: true }));
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
