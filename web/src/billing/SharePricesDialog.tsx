import { useState } from "react";
import { Check, Copy, Search, Share2, ShieldCheck } from "lucide-react";

import { Button, ChoicePicker, Input, Modal, cx } from "../ds";
import { strings } from "../i18n";
import { PriceConnectionCloseButton } from "./PriceConnectionCloseButton";
import { PriceConnectionField } from "./PriceConnectionField";
import type { PriceConnection } from "./priceConnectionsModel";
import type { BillingProduct } from "./types";

interface SharePricesDialogProps {
  onClose: () => void;
  onShared: (connection: PriceConnection) => void;
  products: BillingProduct[];
  productsLoading: boolean;
}

export function SharePricesDialog({ onClose, onShared, products, productsLoading }: SharePricesDialogProps) {
  const [company, setCompany] = useState("");
  const [delivery, setDelivery] = useState("alo");
  const [catalogue, setCatalogue] = useState("all");
  const [productSearch, setProductSearch] = useState("");
  const [selectedProductIds, setSelectedProductIds] = useState<Set<string>>(() => new Set());
  const [created, setCreated] = useState(false);
  const invite = "https://alo.example/connect/prices/AL7K-Q9M2";
  const key = "alo_live_7kq9d4w6f8n3m2";
  const sharedProducts = catalogue === "all" ? products : products.filter((product) => selectedProductIds.has(product.id));
  const visibleProducts = products.filter((product) => product.name.toLocaleLowerCase().includes(productSearch.trim().toLocaleLowerCase()));
  const canCreate = company.trim() !== "" && sharedProducts.length > 0 && !productsLoading;

  function toggleProduct(id: string) {
    setSelectedProductIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function finish() {
    onShared({
      id: `shared-${Date.now()}`,
      direction: "shared",
      company: company.trim(),
      catalogue: catalogue === "all" ? strings.billingConnectionsLivePriceListAutomatic : strings.billingConnectionsSelectedPriceItems,
      items: sharedProducts.length,
      health: "connected",
      detail: delivery === "alo" ? strings.billingConnectionsWaitingClient : strings.billingConnectionsExternalReady,
      updated: strings.billingConnectionsCreatedNow,
      cadence: delivery === "alo" ? strings.billingConnectionsOnApproval : strings.billingConnectionsLive,
      channel: delivery === "alo" ? "alo" : "api",
    });
  }

  return (
    <Modal
      title={strings.billingConnectionsSharePrices}
      onClose={onClose}
      wide
      icon={<Share2 className="size-5" />}
      actions={<PriceConnectionCloseButton onClick={onClose} />}
      footer={<div className="ml-auto flex gap-3"><Button variant="ghost" onClick={onClose}>{created ? strings.close : strings.cancel}</Button>{!created && <Button disabled={!canCreate} onClick={() => setCreated(true)}>{strings.billingConnectionsCreateSecure}</Button>}{created && <Button onClick={finish}>{strings.quoteStudioDone}</Button>}</div>}
    >
      {!created ? (
        <>
          <div className="rounded-xl border border-accent/20 bg-accent-soft p-4"><p className="m-0 text-sm font-semibold text-primary">{strings.billingConnectionsYouControl}</p><p className="mb-0 mt-1 text-sm leading-relaxed text-secondary">{strings.billingConnectionsYouControlHelp}</p></div>
          <div className="grid grid-cols-2 gap-5 max-md:grid-cols-1">
            <PriceConnectionField label={strings.billingConnectionsClientPartner}><Input value={company} onChange={(event) => setCompany(event.target.value)} placeholder={strings.billingConnectionsCompanyName} /></PriceConnectionField>
            <PriceConnectionField label={strings.billingConnectionsDeliveryMethod}><ChoicePicker value={delivery} label={strings.billingConnectionsDeliveryMethod} placeholder={strings.billingConnectionsChooseDelivery} options={[{ value: "alo", label: strings.billingConnectionsInviteAloWorkspace }, { value: "api", label: strings.billingConnectionsGiveExternalApi }]} onChange={setDelivery} /></PriceConnectionField>
          </div>
          <PriceConnectionField label={strings.billingConnectionsPricesToShare}><ChoicePicker value={catalogue} label={strings.billingConnectionsPricesToShare} placeholder={strings.billingConnectionsChoosePrices} options={[{ value: "all", label: strings.billingConnectionsLivePriceListActive(products.length) }, { value: "selected", label: strings.billingConnectionsChooseProductsSelected(selectedProductIds.size) }]} onChange={setCatalogue} /></PriceConnectionField>
          {catalogue === "selected" && (
            <section className="rounded-xl border border-default bg-raised/30 p-3" aria-label={strings.billingConnectionsChooseProducts}>
              <div className="relative"><Search className="pointer-events-none absolute left-3.5 top-1/2 size-4 -translate-y-1/2 text-tertiary" aria-hidden="true" /><Input className="!pl-10" value={productSearch} onChange={(event) => setProductSearch(event.target.value)} placeholder={strings.billingConnectionsSearchPriceList} aria-label={strings.billingConnectionsSearchPriceList} /></div>
              <div className="mt-3 grid max-h-52 grid-cols-2 gap-2 overflow-y-auto pr-1 max-md:grid-cols-1">
                {visibleProducts.map((product) => {
                  const selected = selectedProductIds.has(product.id);
                  return <button key={product.id} type="button" aria-pressed={selected} className={cx("flex min-h-11 items-center gap-3 rounded-lg border px-3 py-2 text-left text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/30", selected ? "border-accent/30 !bg-accent-soft !text-accent" : "border-default !bg-surface text-primary hover:!border-accent/30 hover:!bg-accent-soft hover:!text-accent")} onClick={() => toggleProduct(product.id)}><span className={cx("inline-flex size-5 shrink-0 items-center justify-center rounded-md border", selected ? "border-accent bg-accent text-on-accent" : "border-strong bg-surface")}>{selected && <Check className="size-3.5" aria-hidden="true" />}</span><span className="min-w-0 flex-1 truncate font-medium">{product.name}</span><span className="shrink-0 text-xs text-tertiary">{product.unit || strings.billingConnectionsItemUnit}</span></button>;
                })}
                {!productsLoading && visibleProducts.length === 0 && <p className="col-span-full m-0 px-2 py-4 text-center text-sm text-secondary">{strings.billingConnectionsNoProducts}</p>}
                {productsLoading && <p className="col-span-full m-0 px-2 py-4 text-center text-sm text-secondary">{strings.billingConnectionsLoadingPriceList}</p>}
              </div>
            </section>
          )}
          <div className="grid grid-cols-3 gap-3 max-md:grid-cols-1">{[[strings.billingConnectionsPrices, catalogue === "all" ? strings.billingConnectionsLivePriceListCount(products.length) : strings.billingConnectionsSelectedProductsCount(selectedProductIds.size)], [strings.billingConnectionsUpdates, strings.billingConnectionsChangesFlow], [strings.billingConnectionsValidity, strings.billingConnectionsNoExpiry]].map(([label, value]) => <div key={label} className="rounded-xl border border-default bg-raised/40 p-4"><p className="m-0 text-xs font-semibold uppercase tracking-wide text-tertiary">{label}</p><p className="mb-0 mt-1.5 text-sm font-medium text-primary">{value}</p></div>)}</div>
        </>
      ) : (
        <div className="flex flex-col gap-5">
          <div className="flex items-start gap-3 rounded-xl border border-success/20 bg-success-tint p-4"><ShieldCheck className="mt-0.5 size-5 shrink-0 text-success" /><div><p className="m-0 text-sm font-semibold text-primary">{strings.billingConnectionsSecureCreated}</p><p className="mb-0 mt-1 text-sm text-secondary">{strings.billingConnectionsSendTo(company)}</p></div></div>
          <PriceConnectionField label={delivery === "alo" ? strings.billingConnectionsAloInvitationLink : strings.billingConnectionsPriceApiAddress}><div className="flex gap-2"><Input readOnly value={delivery === "alo" ? invite : "https://api.alo.example/v1/shared-prices/AL7K-Q9M2"} /><Button variant="ghost" icon={<Copy />} onClick={() => void navigator.clipboard?.writeText(delivery === "alo" ? invite : "https://api.alo.example/v1/shared-prices/AL7K-Q9M2")}>{strings.billingConnectionsCopy}</Button></div></PriceConnectionField>
          {delivery === "api" && <PriceConnectionField label={strings.billingConnectionsAccessKey} hint={strings.billingConnectionsKeyShownOnce}><div className="flex gap-2"><Input readOnly value={key} /><Button variant="ghost" icon={<Copy />} onClick={() => void navigator.clipboard?.writeText(key)}>{strings.billingConnectionsCopy}</Button></div></PriceConnectionField>}
        </div>
      )}
    </Modal>
  );
}
