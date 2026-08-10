// What a document is assembled from: the customer it is made out to, and the
// price-list items its lines can be picked from. Both editors need exactly
// this pair, so it is loaded in one place rather than in each of them.
//
// Archived customers are loaded on purpose. A document already raised for a
// customer who has since been archived still has to name them; the picker is
// what filters the archived ones out of the *choices*, so that a record can
// always show who it is for without offering them for new business.
import { useEffect, useState } from "react";

import { strings } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import type { BillingCustomer, BillingInvoiceSummary, BillingProduct } from "./types";

export interface Pickers {
  /** Every customer, archived included. */
  customers: BillingCustomer[];
  /** The active price list; archived items are not offered on a new line. */
  products: BillingProduct[];
  /** Why they could not be loaded — an editor whose pickers are empty must
   *  say so rather than look like a tenant with no customers. */
  error: string | null;
}

/** The customers and the price list, loaded once per screen. */
export function usePickers(): Pickers {
  const api = useBillingApi();
  const [customers, setCustomers] = useState<BillingCustomer[]>([]);
  const [products, setProducts] = useState<BillingProduct[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const [people, catalogue] = await Promise.all([api.customers(true), api.products()]);
        if (!live) return;
        setCustomers(people);
        setProducts(catalogue);
        setError(null);
      } catch (err) {
        if (live) setError(billingMessage(err, strings.billingLoadFailed));
      }
    })();
    return () => {
      live = false;
    };
  }, [api]);

  return { customers, products, error };
}

/**
 * The documents that can still take money, for a screen outside Billing that
 * has to name one — the reconciliation screen in Finance (B4.13b), where a
 * person says by hand which invoice a bank line settled.
 *
 * Issued, unsettled, and not a credit note. All three are the server's own
 * facts; this hook narrows the list it was given and never works out what is
 * owed, which arrives as `settlement.outstandingCents` beside each one.
 *
 * `enabled` exists because the caller is a dialog: a picker nobody has opened
 * must not read the whole ledger on every render of the screen behind it.
 */
export function useOpenInvoices(enabled = true): {
  invoices: BillingInvoiceSummary[];
  error: string | null;
  loading: boolean;
} {
  const api = useBillingApi();
  const [invoices, setInvoices] = useState<BillingInvoiceSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!enabled) return;
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const issued = await api.invoices("issued");
        if (!live) return;
        // Two categorical filters over the server's own answers, and no
        // arithmetic: `settlement.state` is computed on the server from the
        // payment rows, and `creditNote` is a flag it stores. A settled
        // document and a credit note are both refusals on the settling route,
        // and offering a choice that always fails is worse than not offering
        // it.
        setInvoices(
          issued.filter((invoice) => !invoice.creditNote && invoice.settlement.state !== "paid"),
        );
        setError(null);
      } catch (err) {
        if (live) setError(billingMessage(err, strings.billingLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, enabled]);

  return { invoices, error, loading };
}

/**
 * Who a tenant bills, for a screen outside Billing that has to name one — the
 * engagement list and the engagement form in Projects (B3.07).
 *
 * `includeArchived` is the same split `usePickers` makes and for the same
 * reason: a **picker** must not offer an archived customer, because attaching
 * new work to one is a refusal on the server and offering a choice that always
 * fails is worse than not offering it. A **list** must include them, because a
 * project attached to a customer who has since been archived still has to say
 * whose work it is.
 */
export function useCustomers(includeArchived = false): {
  customers: BillingCustomer[];
  error: string | null;
} {
  const api = useBillingApi();
  const [customers, setCustomers] = useState<BillingCustomer[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const people = await api.customers(includeArchived);
        if (live) {
          setCustomers(people);
          setError(null);
        }
      } catch (err) {
        if (live) setError(billingMessage(err, strings.billingLoadFailed));
      }
    })();
    return () => {
      live = false;
    };
  }, [api, includeArchived]);

  return { customers, error };
}
