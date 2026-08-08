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
import type { BillingCustomer, BillingProduct } from "./types";

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
