import { ChevronLeft, ChevronRight } from "lucide-react";

import { strings } from "../i18n";

interface BillingPaginationProps {
  first: number;
  last: number;
  total: number;
  page: number;
  pageCount: number;
  onPage: (page: number) => void;
}

export function BillingPagination({ first, last, total, page, pageCount, onPage }: BillingPaginationProps) {
  if (total <= 25) return null;
  return <nav className="flex flex-wrap items-center justify-between gap-3 border-t border-subtle px-4 py-3" aria-label={strings.billingPaginationLabel}>
    <p className="m-0 text-sm text-secondary">{strings.billingPaginationRange(first, last, total)}</p>
    <div className="flex items-center gap-2">
      <button type="button" className="inline-flex size-9 items-center justify-center rounded-lg text-secondary transition-colors hover:bg-accent-soft hover:text-accent disabled:pointer-events-none disabled:opacity-40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30" aria-label={strings.billingPaginationPrevious} disabled={page === 1} onClick={() => onPage(page - 1)}><ChevronLeft className="size-4" aria-hidden="true" /></button>
      <span className="min-w-20 text-center text-sm font-medium text-primary">{strings.billingPaginationPage(page, pageCount)}</span>
      <button type="button" className="inline-flex size-9 items-center justify-center rounded-lg text-secondary transition-colors hover:bg-accent-soft hover:text-accent disabled:pointer-events-none disabled:opacity-40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30" aria-label={strings.billingPaginationNext} disabled={page === pageCount} onClick={() => onPage(page + 1)}><ChevronRight className="size-4" aria-hidden="true" /></button>
    </div>
  </nav>;
}
