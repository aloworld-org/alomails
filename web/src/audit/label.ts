// Turning a server action into a sentence (alo audit trail, wave B2.13).
//
// The log speaks a dotted vocabulary — `billing.invoice.payment.create` — that
// is deliberately stable, machine-readable and never shown raw. What a person
// reads is the **verb**, because the record kind is already the page they are
// on: an invoice's history says "Issued", not "Invoice issued".
//
// The verb is what is left after the record kind, so one label serves every
// module: `create` is "Created" on a customer, a deal and a quote alike. An
// action nobody has written a label for falls back to the raw verb rather than
// to English invented here — a missing translation should look like a missing
// translation, not like a different act.
import { strings } from "../i18n";

/** The part of an action that is not the record kind:
 *  `billing.invoice.payment.create` → `payment.create`. */
export function verbOf(action: string, entityType: string | null): string {
  if (entityType !== null && entityType !== "" && action.startsWith(`${entityType}.`)) {
    return action.slice(entityType.length + 1);
  }
  // No entity type to lean on (an administrative entry): the kind is the first
  // two components by construction, so whatever follows them is the verb.
  const parts = action.split(".");
  return parts.length > 2 ? parts.slice(2).join(".") : action;
}

/** The verb vocabulary, read at call time so a language switch reaches it. */
function labels(): Record<string, string> {
  return {
    create: strings.auditActionCreate,
    update: strings.auditActionUpdate,
    delete: strings.auditActionDelete,
    archive: strings.auditActionArchive,
    issue: strings.auditActionIssue,
    void: strings.auditActionVoid,
    credit_note: strings.auditActionCreditNote,
    send: strings.auditActionSend,
    reminder: strings.auditActionReminder,
    "payment.create": strings.auditActionPaymentCreate,
    "payment.delete": strings.auditActionPaymentDelete,
    import: strings.auditActionImport,
    sepa_xml: strings.auditActionSepaXml,
    approve: strings.auditActionApprove,
    reject: strings.auditActionReject,
    accept: strings.auditActionAccept,
    decline: strings.auditActionDecline,
    expire: strings.auditActionExpire,
    run: strings.auditActionRun,
    pause: strings.auditActionPause,
    resume: strings.auditActionResume,
    "rates.update": strings.auditActionRatesUpdate,
    "rates.import": strings.auditActionRatesImport,
    stage: strings.auditActionStageMove,
    "stage.create": strings.auditActionStageCreate,
    move: strings.auditActionMove,
    quote: strings.auditActionQuoteRaised,
    invoice: strings.auditActionInvoiceRaised,
    "activity.create": strings.auditActionActivityCreate,
    "next_step.create": strings.auditActionNextStepCreate,
    "thread.create": strings.auditActionThreadCreate,
    "thread.delete": strings.auditActionThreadDelete,
    "lead.create": strings.auditActionLeadCreate,
  };
}

/** What one entry did, in the reader's language. */
export function actionLabel(action: string, entityType: string | null): string {
  const verb = verbOf(action, entityType);
  return labels()[verb] ?? verb;
}

/** The person who did it, or the stand-in for an account that no longer
 *  exists — never a raw user id, which names nobody. */
export function actorLabel(actor: string | null): string {
  return actor !== null && actor !== "" ? actor : strings.auditUnknownActor;
}
