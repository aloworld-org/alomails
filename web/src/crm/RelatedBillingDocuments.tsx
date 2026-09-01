import { useCallback, useEffect, useState } from "react";
import { FileText, Receipt } from "lucide-react";
import { Link } from "react-router-dom";

import { quoteStatusLabel, statusLabel } from "../billing/statusLogic";
import type { InvoiceStatus, QuoteStatus } from "../billing/types";
import { Badge, Spinner } from "../ds";
import { strings } from "../i18n";
import { crmMessage, useCrmApi } from "./api";
import type { DealBillingDocument } from "./types";

function label(document: DealBillingDocument): string {
  const fallback =
    document.kind === "quote" ? strings.crmDocumentQuote : strings.crmDocumentInvoice;
  return document.number === null ? fallback : document.number;
}

function status(document: DealBillingDocument): string {
  return document.kind === "quote"
    ? quoteStatusLabel(document.status as QuoteStatus)
    : statusLabel(document.status as InvoiceStatus);
}

/** Billing records explicitly raised from this deal; never customer guesses. */
export function RelatedBillingDocuments({
  dealId,
  revision,
}: {
  dealId: string;
  revision: number;
}) {
  const api = useCrmApi();
  const [documents, setDocuments] = useState<DealBillingDocument[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setDocuments(await api.billingDocuments(dealId));
      setError(null);
    } catch (reason) {
      setError(crmMessage(reason, strings.crmRelatedBillingLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, dealId]);

  useEffect(() => {
    void revision;
    void load();
  }, [load, revision]);

  return (
    <section className="rounded-xl border border-subtle bg-surface p-4 shadow-sm">
      <div className="flex items-center gap-2">
        <Receipt size={17} className="text-accent" aria-hidden="true" />
        <h3 className="m-0 text-sm font-semibold text-primary">
          {strings.crmRelatedBilling}
        </h3>
        {loading && <Spinner size={14} />}
        {!loading && documents.length > 0 && (
          <Badge tone="neutral">{documents.length}</Badge>
        )}
      </div>
      {error !== null && (
        <p className="mb-0 mt-3 text-sm text-danger" role="alert">
          {error}
        </p>
      )}
      {!loading && error === null && documents.length === 0 && (
        <p className="mb-0 mt-2 text-sm text-tertiary">
          {strings.crmRelatedBillingEmpty}
        </p>
      )}
      {documents.length > 0 && (
        <ul className="mb-0 mt-3 flex list-none flex-col gap-2 p-0">
          {documents.map((document) => {
            const path = document.kind === "quote" ? "quotes" : "invoices";
            return (
              <li key={`${document.kind}:${document.documentId}`}>
                <Link
                  className="flex min-h-11 items-center gap-3 rounded-lg border border-subtle px-3 py-2 !no-underline hover:bg-raised hover:!no-underline"
                  to={`/billing/${path}/${encodeURIComponent(document.documentId)}`}
                >
                  <span className="rounded-md bg-secondary p-1.5 text-secondary">
                    <FileText size={15} />
                  </span>
                  <span className="min-w-0 flex-1 truncate text-sm font-semibold text-primary">
                    {label(document)}
                  </span>
                  <span className="text-xs text-tertiary">{status(document)}</span>
                </Link>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
