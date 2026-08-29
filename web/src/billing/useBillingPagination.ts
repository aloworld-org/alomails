import { useEffect, useMemo, useState } from "react";

const BILLING_PAGE_SIZE = 25;

export function useBillingPagination<T>(records: readonly T[], resetKey: string) {
  const [page, setPage] = useState(1);
  const pageCount = Math.max(1, Math.ceil(records.length / BILLING_PAGE_SIZE));

  useEffect(() => setPage(1), [resetKey]);
  useEffect(() => setPage((current) => Math.min(current, pageCount)), [pageCount]);

  return useMemo(() => {
    const safePage = Math.min(page, pageCount);
    const firstIndex = (safePage - 1) * BILLING_PAGE_SIZE;
    return {
      records: records.slice(firstIndex, firstIndex + BILLING_PAGE_SIZE),
      page: safePage,
      pageCount,
      setPage,
      first: records.length === 0 ? 0 : firstIndex + 1,
      last: Math.min(firstIndex + BILLING_PAGE_SIZE, records.length),
      total: records.length,
    };
  }, [page, pageCount, records]);
}
