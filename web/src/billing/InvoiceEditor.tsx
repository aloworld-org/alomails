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
import { useCallback, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";

import { RecordHistory } from "../audit";
import { strings, useLocale } from "../i18n";
import { useBillingApi } from "./api";
import { formatDocumentDate } from "./dates";
import type { DocumentAction } from "./DocumentActions";
import { DocumentEditor } from "./DocumentEditor";
import type { DocumentHeader, DocumentPatch } from "./documentDraft";
import { useDocumentDraft } from "./documentDraft";
import { Field } from "./parts";
import { PaymentsPanel } from "./PaymentsPanel";
import { usePickers } from "./pickers";
import { ScheduleDialog } from "./ScheduleDialog";
import { DocumentChips } from "./status";
import type { BillingInvoice, BillingInvoiceSummary } from "./types";
import styles from "./billingStyles";

export function InvoiceEditor() {
  const { id } = useParams<{ id: string }>();
  const api = useBillingApi();
  const locale = useLocale();
  const location = useLocation();
  const navigate = useNavigate();
  const pickers = usePickers();
  const locationState = location.state as {
    fromProject?: { id?: unknown; name?: unknown };
  } | null;
  const fromProject = locationState?.fromProject;
  const projectOrigin =
    typeof fromProject?.id === "string" &&
    fromProject.id !== "" &&
    typeof fromProject.name === "string" &&
    fromProject.name !== ""
      ? { id: fromProject.id, name: fromProject.name }
      : null;

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
  // The payment ledger is the panel's own read (`PaymentsPanel`), not part of
  // the draft: it changes for its own reasons — recording money never edits the
  // document — and loading it here would make every autosave refetch it.
  const create = useCallback(
    (header: Partial<DocumentHeader>) => api.createInvoice(header),
    [api],
  );
  const save = useCallback(
    (documentId: string, patch: DocumentPatch) => api.updateInvoice(documentId, patch),
    [api],
  );
  const editable = useCallback((invoice: BillingInvoice) => invoice.status === "draft", []);

  // Whether the "repeat this invoice" form is open. The arrangement it sets up
  // takes its customer, currency, terms and lines from THIS document, which is
  // why the entry point lives on the document rather than on the recurring
  // list — there is no second line editor to keep in step.
  const [repeating, setRepeating] = useState(false);

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
          const issued = await api.issueInvoice(id);
          draft.adopt(issued);
          const mail = await api.sendInvoice(id);
          await navigate(`/mail?open=${encodeURIComponent(mail.id)}`);
        },
      });
    }
    // A document the customer may already hold is corrected, not cancelled —
    // so crediting leads and voiding is the quiet option beside it.
    if (invoice.status === "issued" || invoice.status === "paid") {
      actions.push({
        key: "prepare-email",
        label: strings.billingPrepareInvoiceEmail,
        title: strings.billingPrepareInvoiceEmailTitle,
        message: strings.billingPrepareInvoiceEmailConfirm,
        primary: true,
        run: async () => {
          const mail = await api.sendInvoice(id);
          await navigate(`/mail?open=${encodeURIComponent(mail.id)}`);
        },
      });
      actions.push({
        key: "credit-note",
        label: strings.billingCreditNoteAction,
        title: strings.billingCreditNoteTitle,
        message: strings.billingCreditNoteConfirm,
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
        back:
          projectOrigin === null
            ? strings.billingBackToInvoices
            : strings.billingBackToProject(projectOrigin.name),
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
            {/* Offered on a document that says something and is settled in
                what it is: a draft with no lines has nothing to repeat, and a
                credit note is a correction, not an arrangement. The document a
                colleague points at is the template, so the button sits with the
                document rather than on the recurring list. */}
            {!invoice.creditNote && invoice.lines.length > 0 && (
              <p className={styles.relation}>
                <button
                  type="button"
                  className={styles.linkAction}
                  title={strings.billingScheduleFromHint}
                  onClick={() => setRepeating(true)}
                >
                  {strings.billingScheduleFrom}
                </button>
              </p>
            )}
            {repeating && (
              <ScheduleDialog
                schedule={null}
                from={invoice}
                {...(invoice.reference !== ""
                  ? { suggestedName: invoice.reference }
                  : invoice.number !== null
                    ? { suggestedName: invoice.number }
                    : {})}
                onClose={() => setRepeating(false)}
                onSaved={() => {
                  setRepeating(false);
                  void navigate("../../recurring");
                }}
              />
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
            {/* Money is only ever received against a document the customer
                holds and owes. A credit note is the other direction, and a
                draft is owed by nobody — the store refuses both, and the panel
                is not offered on them rather than showing a button that 409s. */}
            {!invoice.creditNote &&
              (invoice.status === "issued" || invoice.status === "paid") &&
              id !== undefined && (
                <PaymentsPanel
                  invoiceId={id}
                  currency={invoice.currency}
                  settlement={invoice.settlement}
                  onInvoiceChanged={draft.adopt}
                />
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
            {/* Who did what to this invoice, and when (B2.13). Below the
                document rather than beside it: a bookkeeper reads the numbers
                first and the history only when something looks wrong. A draft
                that was never saved has no id and therefore no history. */}
            {id !== undefined && <RecordHistory entityType="billing.invoice" entityId={id} />}
          </>
        )
      }
      onBack={() => {
        if (projectOrigin !== null) {
          void navigate(
            `/projects/${encodeURIComponent(projectOrigin.id)}/overview`,
          );
          return;
        }
        void navigate("..");
      }}
      onCreated={(created) => {
        // Replaces the /new entry, so Back goes to the list rather than to a
        // form for a document that now exists.
        void navigate(`../${created.id}`, { replace: true });
      }}
      onPrint={id === undefined ? undefined : () => api.documentHtml("invoices", id)}
      onDiscard={async () => {
        if (id === undefined) return;
        await api.deleteInvoice(id);
        await navigate("..");
      }}
    />
  );
}
