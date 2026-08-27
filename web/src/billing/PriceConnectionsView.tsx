import { useEffect, useMemo, useState } from "react";
import { ArrowDownToLine, ArrowUpFromLine, Check, Link2, Search, Share2, X } from "lucide-react";

import { Button, Input, cx, useDialogs } from "../ds";
import { strings } from "../i18n";
import { useBillingApi } from "./api";
import { ConnectSupplierDialog } from "./ConnectSupplierDialog";
import { PriceConnectionCard } from "./PriceConnectionCard";
import {
  PRICE_CONNECTIONS_STORAGE_KEY,
  loadStoredPriceConnections,
  type PriceConnection,
  type PriceConnectionDirection,
} from "./priceConnectionsModel";
import { SharePricesDialog } from "./SharePricesDialog";
import type { BillingProduct } from "./types";

export function PriceConnectionsView() {
  const { confirm } = useDialogs();
  const api = useBillingApi();
  const [direction, setDirection] = useState<PriceConnectionDirection>("received");
  const [connections, setConnections] = useState<PriceConnection[]>(loadStoredPriceConnections);
  const [search, setSearch] = useState("");
  const [dialog, setDialog] = useState<PriceConnectionDirection | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [products, setProducts] = useState<BillingProduct[]>([]);
  const [productsLoading, setProductsLoading] = useState(true);
  const shown = useMemo(() => {
    const needle = search.trim().toLocaleLowerCase();
    return connections.filter((connection) => connection.direction === direction && (needle === "" || `${connection.company} ${connection.catalogue}`.toLocaleLowerCase().includes(needle)));
  }, [connections, direction, search]);

  useEffect(() => {
    try {
      localStorage.setItem(PRICE_CONNECTIONS_STORAGE_KEY, JSON.stringify(connections));
    } catch {
      // The workspace remains usable when private browsing blocks storage.
    }
  }, [connections]);

  useEffect(() => {
    let active = true;
    setProductsLoading(true);
    void api.products(false)
      .then((items) => {
        if (active) setProducts(items.filter((item) => !item.archived));
      })
      .catch(() => {
        if (active) setProducts([]);
      })
      .finally(() => {
        if (active) setProductsLoading(false);
      });
    return () => { active = false; };
  }, [api]);

  function add(connection: PriceConnection) {
    setConnections((current) => [connection, ...current]);
    setDirection(connection.direction);
    setDialog(null);
    setNotice(connection.direction === "received" ? strings.billingConnectionsNowSupplying(connection.company) : strings.billingConnectionsNowReceiving(connection.company));
  }

  return (
    <div className="mx-auto flex min-h-0 w-full max-w-[112rem] flex-1 flex-col gap-4 overflow-y-auto px-8 pb-8 pt-6 max-[52rem]:p-4">
      <section className="flex flex-wrap items-center gap-4 px-1 py-1">
        <div className="min-w-0 flex-1"><h2 className="m-0 text-xl font-semibold tracking-tight text-primary">{strings.billingConnectionsTitle}</h2><p className="mb-0 mt-1 text-sm leading-relaxed text-secondary">{strings.billingConnectionsSubtitle}</p></div>
        <div className="flex flex-wrap gap-2"><Button variant="ghost" icon={<ArrowDownToLine />} onClick={() => setDialog("received")}>{strings.billingConnectionsConnectSupplier}</Button><Button icon={<Share2 />} onClick={() => setDialog("shared")}>{strings.billingConnectionsSharePrices}</Button></div>
      </section>

      <section className="flex flex-wrap items-center gap-4 rounded-xl border border-default bg-surface p-3 shadow-sm">
        <div className="inline-flex gap-1" role="tablist" aria-label={strings.billingConnectionsDirection}>
          {(["received", "shared"] as const).map((tab) => (
            <button key={tab} type="button" role="tab" aria-selected={direction === tab} className={cx("inline-flex min-h-10 items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30", direction === tab ? "bg-accent-soft text-accent" : "text-secondary hover:bg-raised hover:text-primary")} onClick={() => setDirection(tab)}>
              {tab === "received" ? <ArrowDownToLine className="size-4" /> : <ArrowUpFromLine className="size-4" />}
              {tab === "received" ? strings.billingConnectionsReceivedByMe : strings.billingConnectionsSharedByMe}
            </button>
          ))}
        </div>
        <label className="relative ml-auto flex min-w-64 items-center max-sm:ml-0 max-sm:w-full"><Search className="pointer-events-none absolute left-3.5 size-4 text-tertiary" /><Input type="search" value={search} onChange={(event) => setSearch(event.target.value)} className="!pl-10" placeholder={strings.billingConnectionsSearch} aria-label={strings.billingConnectionsSearch} /></label>
      </section>

      {notice !== null && <div className="flex items-center gap-3 rounded-xl border border-success/20 bg-success-tint px-4 py-3 text-sm text-primary" role="status"><Check className="size-4 shrink-0 text-success" />{notice}<button type="button" className="ml-auto rounded-lg p-2 text-tertiary hover:bg-surface hover:text-primary" aria-label={strings.billingConnectionsDismiss} onClick={() => setNotice(null)}><X className="size-4" /></button></div>}

      <div className="grid gap-4">
        {shown.map((connection) => <PriceConnectionCard key={connection.id} connection={connection} onSync={() => { setConnections((current) => current.map((item) => item.id === connection.id ? { ...item, updated: strings.billingConnectionsUpdatedNow, health: "connected" } : item)); setNotice(strings.billingConnectionsUpToDate(connection.company)); }} onToggle={() => setConnections((current) => current.map((item) => item.id === connection.id ? { ...item, health: item.health === "paused" ? "connected" : "paused" } : item))} onRemove={() => { void (async () => {
          const accepted = await confirm({
            title: strings.billingConnectionsDisconnectTitle,
            message: connection.direction === "received" ? strings.billingConnectionsDisconnectReceived(connection.company) : strings.billingConnectionsDisconnectShared(connection.company),
            confirmLabel: strings.billingConnectionsDisconnect,
            cancelLabel: strings.billingConnectionsKeepConnected,
            danger: true,
          });
          if (accepted) setConnections((current) => current.filter((item) => item.id !== connection.id));
        })(); }} />)}
        {shown.length === 0 && <div className="flex min-h-56 flex-col items-center justify-center rounded-2xl border border-dashed border-default bg-surface p-8 text-center"><Link2 className="size-8 text-accent" /><h3 className="mb-0 mt-3 text-base font-semibold text-primary">{strings.billingConnectionsNoMatches}</h3><p className="mb-0 mt-1 text-sm text-secondary">{strings.billingConnectionsNoMatchesHelp}</p></div>}
      </div>

      {dialog === "received" && <ConnectSupplierDialog onClose={() => setDialog(null)} onConnected={add} />}
      {dialog === "shared" && <SharePricesDialog products={products} productsLoading={productsLoading} onClose={() => setDialog(null)} onShared={add} />}
    </div>
  );
}
