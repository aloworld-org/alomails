import { useState } from "react";
import { ArrowDownToLine, Check, ChevronDown, RefreshCw } from "lucide-react";

import { Button, ChoicePicker, Input, Modal } from "../ds";
import { strings } from "../i18n";
import { PriceConnectionCloseButton } from "./PriceConnectionCloseButton";
import { PriceConnectionField } from "./PriceConnectionField";
import type { PriceConnection } from "./priceConnectionsModel";

export function ConnectSupplierDialog({ onClose, onConnected }: { onClose: () => void; onConnected: (connection: PriceConnection) => void }) {
  const [company, setCompany] = useState("");
  const [source, setSource] = useState("alo");
  const [address, setAddress] = useState("");
  const [accessKey, setAccessKey] = useState("");
  const [tested, setTested] = useState(false);
  const [schedule, setSchedule] = useState("daily");
  const [changePolicy, setChangePolicy] = useState("review");
  const [changeLimit, setChangeLimit] = useState("5");
  const [matching, setMatching] = useState("sku");
  const [newProducts, setNewProducts] = useState("review");
  const [skuField, setSkuField] = useState("sku");
  const [nameField, setNameField] = useState("description");
  const [priceField, setPriceField] = useState("net_price");
  const [currencyField, setCurrencyField] = useState("currency");
  const [headerName, setHeaderName] = useState("");
  const [headerValue, setHeaderValue] = useState("");
  const canTest = company.trim() !== "" && address.trim() !== "";

  return <Modal title={strings.billingConnectionsConnectSupplier} onClose={onClose} wide icon={<ArrowDownToLine className="size-5" />} actions={<PriceConnectionCloseButton onClick={onClose} />} footer={<div className="ml-auto flex gap-3"><Button variant="ghost" onClick={onClose}>{strings.cancel}</Button><Button disabled={!tested} onClick={() => onConnected({ id: `received-${Date.now()}`, direction: "received", company: company.trim(), catalogue: strings.billingConnectionsSupplierCatalogueEur, items: 148, health: "connected", detail: strings.billingConnectionsNoChangesAttention, updated: strings.billingConnectionsConnectedNow, cadence: schedule === "hourly" ? strings.billingConnectionsHourly : schedule === "weekly" ? strings.billingConnectionsWeekly : schedule === "manual" ? strings.billingConnectionsManual : strings.billingConnectionsDaily, channel: source === "alo" ? "alo" : "api" })}>{strings.billingConnectionsConnectPrices}</Button></div>}>
    <div className="rounded-xl border border-accent/20 bg-accent-soft p-4"><p className="m-0 text-sm font-semibold text-primary">{strings.billingConnectionsEasyOption}</p><p className="mb-0 mt-1 text-sm leading-relaxed text-secondary">{strings.billingConnectionsEasyOptionHelp}</p></div>
    <div className="grid grid-cols-2 gap-5 max-md:grid-cols-1">
      <PriceConnectionField label={strings.billingConnectionsSupplier}><Input value={company} onChange={(event) => { setCompany(event.target.value); setTested(false); }} placeholder={strings.billingConnectionsSupplierPlaceholder} /></PriceConnectionField>
      <PriceConnectionField label={strings.billingConnectionsType}><ChoicePicker value={source} label={strings.billingConnectionsType} placeholder={strings.billingConnectionsChooseConnection} options={[{ value: "alo", label: strings.billingConnectionsAloInvitationLink }, { value: "api", label: strings.billingConnectionsExternalPricingApi }, { value: "file", label: strings.billingConnectionsSpreadsheetFeed }]} onChange={(value) => { setSource(value); setTested(false); }} /></PriceConnectionField>
    </div>
    {source === "alo" ? <PriceConnectionField label={strings.billingConnectionsInvitationLink} hint={strings.billingConnectionsInvitationHelp}><Input value={address} onChange={(event) => { setAddress(event.target.value); setTested(false); }} placeholder={strings.billingConnectionsInvitationPlaceholder} /></PriceConnectionField> : <div className="grid grid-cols-2 gap-5 max-md:grid-cols-1"><PriceConnectionField label={source === "api" ? strings.billingConnectionsPriceApiAddress : strings.billingConnectionsFeedAddress} hint={strings.billingConnectionsFormatDetection}><Input value={address} onChange={(event) => { setAddress(event.target.value); setTested(false); }} placeholder={strings.billingConnectionsAddressPlaceholder} /></PriceConnectionField><PriceConnectionField label={strings.billingConnectionsAccessKey} hint={strings.billingConnectionsAccessKeyHelp}><Input type="password" value={accessKey} onChange={(event) => { setAccessKey(event.target.value); setTested(false); }} placeholder={strings.billingConnectionsAccessKeyPlaceholder} /></PriceConnectionField></div>}
    {tested ? <div className="rounded-xl border border-success/20 bg-success-tint p-4"><div className="flex items-center gap-2 text-sm font-semibold text-success"><Check className="size-4" />{strings.billingConnectionsReady}</div><p className="mb-0 mt-1 text-sm text-secondary">{strings.billingConnectionsTestSummary(148, 131, 17)}</p></div> : <Button variant="ghost" icon={<RefreshCw />} disabled={!canTest} onClick={() => setTested(true)}>{strings.billingConnectionsTestPreview}</Button>}
    <details className="group rounded-xl border border-default bg-surface p-4">
      <summary className="flex cursor-pointer list-none items-center gap-2 text-sm font-semibold text-primary"><ChevronDown className="size-4 transition-transform group-open:rotate-180" />{strings.billingConnectionsAdvancedSettings}</summary>
      <div className="mt-4 grid gap-4 border-t border-subtle pt-4">
        <section><h3 className="m-0 text-sm font-semibold text-primary">{strings.billingConnectionsSyncApprovals}</h3><p className="mb-0 mt-1 text-xs leading-relaxed text-tertiary">{strings.billingConnectionsSyncApprovalsHelp}</p><div className="mt-3 grid grid-cols-2 gap-4 max-md:grid-cols-1">
          <PriceConnectionField label={strings.billingConnectionsCheckUpdates}><ChoicePicker value={schedule} label={strings.billingConnectionsCheckUpdates} placeholder={strings.billingConnectionsChooseSchedule} options={[{ value: "hourly", label: strings.billingConnectionsEveryHour }, { value: "daily", label: strings.billingConnectionsOnceDay }, { value: "weekly", label: strings.billingConnectionsOnceWeek }, { value: "manual", label: strings.billingConnectionsManualSync }]} onChange={setSchedule} /></PriceConnectionField>
          <PriceConnectionField label={strings.billingConnectionsApplyChanges}><ChoicePicker value={changePolicy} label={strings.billingConnectionsApplyChanges} placeholder={strings.billingConnectionsChooseApproval} options={[{ value: "review", label: strings.billingConnectionsReviewEveryChange }, { value: "limited", label: strings.billingConnectionsAutomaticLimited }, { value: "automatic", label: strings.billingConnectionsAutomaticAll }]} onChange={setChangePolicy} /></PriceConnectionField>
          {changePolicy === "limited" && <PriceConnectionField label={strings.billingConnectionsChangeLimit} hint={strings.billingConnectionsChangeLimitHelp}><div className="relative"><Input value={changeLimit} onChange={(event) => setChangeLimit(event.target.value)} inputMode="decimal" aria-label={strings.billingConnectionsChangeLimit} className="!pr-10" /><span className="pointer-events-none absolute right-4 top-1/2 -translate-y-1/2 text-sm text-tertiary">%</span></div></PriceConnectionField>}
        </div></section>
        <section className="border-t border-subtle pt-4"><h3 className="m-0 text-sm font-semibold text-primary">{strings.billingConnectionsProductMatching}</h3><p className="mb-0 mt-1 text-xs leading-relaxed text-tertiary">{strings.billingConnectionsProductMatchingHelp}</p><div className="mt-3 grid grid-cols-2 gap-4 max-md:grid-cols-1">
          <PriceConnectionField label={strings.billingConnectionsMatchBy}><ChoicePicker value={matching} label={strings.billingConnectionsMatchBy} placeholder={strings.billingConnectionsChooseMatching} options={[{ value: "sku", label: strings.billingConnectionsMatchSku }, { value: "barcode", label: strings.billingConnectionsMatchBarcode }, { value: "name", label: strings.billingConnectionsMatchName }, { value: "manual", label: strings.billingConnectionsMatchReview }]} onChange={setMatching} /></PriceConnectionField>
          <PriceConnectionField label={strings.billingConnectionsNewProducts}><ChoicePicker value={newProducts} label={strings.billingConnectionsNewProducts} placeholder={strings.billingConnectionsChooseAction} options={[{ value: "review", label: strings.billingConnectionsHoldReview }, { value: "create", label: strings.billingConnectionsCreateDraftItems }, { value: "ignore", label: strings.billingConnectionsDoNotImport }]} onChange={setNewProducts} /></PriceConnectionField>
        </div></section>
        {source !== "alo" && <section className="border-t border-subtle pt-4"><h3 className="m-0 text-sm font-semibold text-primary">{strings.billingConnectionsFieldMapping}</h3><p className="mb-0 mt-1 text-xs leading-relaxed text-tertiary">{strings.billingConnectionsFieldMappingHelp}</p><div className="mt-3 grid grid-cols-4 gap-3 max-lg:grid-cols-2 max-sm:grid-cols-1"><PriceConnectionField label={strings.billingConnectionsSkuField}><Input value={skuField} onChange={(event) => setSkuField(event.target.value)} /></PriceConnectionField><PriceConnectionField label={strings.billingConnectionsNameField}><Input value={nameField} onChange={(event) => setNameField(event.target.value)} /></PriceConnectionField><PriceConnectionField label={strings.billingConnectionsNetPriceField}><Input value={priceField} onChange={(event) => setPriceField(event.target.value)} /></PriceConnectionField><PriceConnectionField label={strings.billingConnectionsCurrencyField}><Input value={currencyField} onChange={(event) => setCurrencyField(event.target.value)} /></PriceConnectionField></div></section>}
        {source === "api" && <section className="border-t border-subtle pt-4"><h3 className="m-0 text-sm font-semibold text-primary">{strings.billingConnectionsCustomHeader}</h3><p className="mb-0 mt-1 text-xs leading-relaxed text-tertiary">{strings.billingConnectionsCustomHeaderHelp}</p><div className="mt-3 grid grid-cols-2 gap-4 max-md:grid-cols-1"><PriceConnectionField label={strings.billingConnectionsHeaderName}><Input value={headerName} onChange={(event) => setHeaderName(event.target.value)} placeholder={strings.billingConnectionsHeaderNamePlaceholder} /></PriceConnectionField><PriceConnectionField label={strings.billingConnectionsHeaderValue}><Input type="password" value={headerValue} onChange={(event) => setHeaderValue(event.target.value)} placeholder={strings.billingConnectionsHeaderValuePlaceholder} /></PriceConnectionField></div></section>}
      </div>
    </details>
  </Modal>;
}
