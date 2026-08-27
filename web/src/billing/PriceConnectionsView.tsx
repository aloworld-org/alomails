import { useEffect, useMemo, useState } from "react";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  Check,
  ChevronDown,
  CircleAlert,
  Clock3,
  Copy,
  Link2,
  Pause,
  Play,
  RefreshCw,
  Search,
  Share2,
  ShieldCheck,
  Unplug,
  X,
} from "lucide-react";

import { Button, ChoicePicker, Input, Modal, cx, useDialogs } from "../ds";
import { useBillingApi } from "./api";
import type { BillingProduct } from "./types";

type Direction = "received" | "shared";
type Health = "connected" | "attention" | "paused";

interface Connection {
  id: string;
  direction: Direction;
  company: string;
  catalogue: string;
  items: number;
  health: Health;
  detail: string;
  updated: string;
  cadence: string;
  changes?: number;
  channel: "alo" | "api";
}

const INITIAL_CONNECTIONS: Connection[] = [
  {
    id: "received-nordwerk",
    direction: "received",
    company: "Nordwerk Components",
    catalogue: "Industrial components · EUR",
    items: 386,
    health: "connected",
    detail: "4 price changes are ready to review",
    updated: "Updated 12 minutes ago",
    cadence: "Daily",
    changes: 4,
    channel: "alo",
  },
  {
    id: "received-rotterdam",
    direction: "received",
    company: "Rotterdam Metals BV",
    catalogue: "Metals and sheet stock · EUR",
    items: 124,
    health: "attention",
    detail: "The supplier needs to renew this connection",
    updated: "Last updated 2 days ago",
    cadence: "Daily",
    channel: "api",
  },
  {
    id: "shared-atlas",
    direction: "shared",
    company: "Atlas Advisory GmbH",
    catalogue: "Wholesale catalogue · Contract pricing",
    items: 82,
    health: "connected",
    detail: "Their alo workspace receives approved price changes",
    updated: "Used 1 hour ago",
    cadence: "On approval",
    channel: "alo",
  },
  {
    id: "shared-harbor",
    direction: "shared",
    company: "Harbor Logistics NV",
    catalogue: "Project supply prices · EUR",
    items: 24,
    health: "connected",
    detail: "External API access expires 30 September 2026",
    updated: "Used yesterday",
    cadence: "Live",
    channel: "api",
  },
];

const STORAGE_KEY = "alo.billing.price-connections.v1";

function storedConnections(): Connection[] {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    if (value === null) return INITIAL_CONNECTIONS;
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) ? (parsed as Connection[]) : INITIAL_CONNECTIONS;
  } catch {
    return INITIAL_CONNECTIONS;
  }
}

const HEALTH = {
  connected: {
    label: "Connected",
    className: "bg-success-tint text-success ring-success/15",
    Icon: Check,
  },
  attention: {
    label: "Action needed",
    className: "bg-danger-tint text-danger ring-danger/15",
    Icon: CircleAlert,
  },
  paused: {
    label: "Paused",
    className: "bg-raised text-secondary ring-default",
    Icon: Pause,
  },
} as const;

function CloseButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      className="inline-flex size-9 items-center justify-center rounded-lg text-tertiary transition-colors hover:bg-raised hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30"
      aria-label="Close"
      onClick={onClick}
    >
      <X className="size-4" aria-hidden="true" />
    </button>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="flex min-w-0 flex-col gap-2">
      <span className="text-xs font-semibold uppercase tracking-wide text-tertiary">{label}</span>
      {children}
      {hint !== undefined && <span className="text-xs leading-relaxed text-tertiary">{hint}</span>}
    </label>
  );
}

function ConnectionCard({
  connection,
  onSync,
  onToggle,
  onRemove,
}: {
  connection: Connection;
  onSync: () => void;
  onToggle: () => void;
  onRemove: () => void;
}) {
  const status = HEALTH[connection.health];
  return (
    <article className="group flex items-center gap-4 rounded-2xl border border-default bg-surface px-5 py-4 shadow-sm transition-[border-color,box-shadow] hover:border-accent/30 hover:shadow-md max-md:flex-wrap">
        <span className="inline-flex size-10 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent">
          {connection.direction === "received" ? (
            <ArrowDownToLine className="size-5" aria-hidden="true" />
          ) : (
            <ArrowUpFromLine className="size-5" aria-hidden="true" />
          )}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <h3 className="m-0 text-base font-semibold text-primary transition-colors group-hover:text-accent">{connection.company}</h3>
            <span className={cx("inline-flex items-center gap-1.5 text-xs font-semibold", connection.health === "connected" ? "text-success" : connection.health === "attention" ? "text-danger" : "text-secondary")}>
              <status.Icon className="size-3.5" aria-hidden="true" />
              {status.label}
            </span>
          </div>
          <p className="mb-0 mt-1 text-sm text-secondary">{connection.catalogue}</p>
          <p className="mb-0 mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-tertiary">
            <span>{connection.items} products</span>
            <span>{connection.cadence} updates</span>
            <span>{connection.channel === "alo" ? "Connected through alo" : "External API"}</span>
            <span className="inline-flex items-center gap-1.5"><Clock3 className="size-3" aria-hidden="true" />{connection.updated}</span>
          </p>
          {connection.health === "attention" && <p className="mb-0 mt-2 text-xs font-medium text-danger">{connection.detail}</p>}
        </div>
        <div className="ml-auto flex shrink-0 items-center gap-2 max-md:ml-14">
        {connection.changes !== undefined && connection.changes > 0 && (
          <Button size="sm">Review {connection.changes} changes</Button>
        )}
        {connection.direction === "received" && (
          <button type="button" className="inline-flex size-10 items-center justify-center rounded-lg text-secondary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30" aria-label="Sync now" title="Sync now" onClick={onSync}><RefreshCw className="size-4" aria-hidden="true" /></button>
        )}
        <button type="button" className="inline-flex size-10 items-center justify-center rounded-lg text-secondary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30" aria-label={connection.health === "paused" ? "Resume" : "Pause"} title={connection.health === "paused" ? "Resume" : "Pause"} onClick={onToggle}>{connection.health === "paused" ? <Play className="size-4" /> : <Pause className="size-4" />}</button>
        <button
          type="button"
          className="inline-flex size-10 items-center justify-center rounded-lg text-tertiary transition-colors hover:bg-danger-tint hover:text-danger focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-danger/25"
          aria-label={`Disconnect ${connection.company}`}
          onClick={onRemove}
        >
          <Unplug className="size-4" aria-hidden="true" />
        </button>
        </div>
    </article>
  );
}

function ConnectSupplierDialog({ onClose, onConnected }: { onClose: () => void; onConnected: (connection: Connection) => void }) {
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

  return (
    <Modal
      title="Connect supplier prices"
      onClose={onClose}
      wide
      icon={<ArrowDownToLine className="size-5" />}
      actions={<CloseButton onClick={onClose} />}
      footer={
        <div className="ml-auto flex gap-3">
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button
            disabled={!tested}
            onClick={() => onConnected({
              id: `received-${Date.now()}`,
              direction: "received",
              company: company.trim(),
              catalogue: "Supplier catalogue · EUR",
              items: 148,
              health: "connected",
              detail: "No price changes need your attention",
              updated: "Connected just now",
              cadence: schedule === "hourly" ? "Hourly" : schedule === "weekly" ? "Weekly" : schedule === "manual" ? "Manual" : "Daily",
              channel: source === "alo" ? "alo" : "api",
            })}
          >
            Connect prices
          </Button>
        </div>
      }
    >
      <div className="rounded-xl border border-accent/20 bg-accent-soft p-4">
        <p className="m-0 text-sm font-semibold text-primary">The easy option comes first</p>
        <p className="mb-0 mt-1 text-sm leading-relaxed text-secondary">If your supplier uses alo, paste their invitation link. We handle authentication and product fields automatically.</p>
      </div>
      <div className="grid grid-cols-2 gap-5 max-md:grid-cols-1">
        <Field label="Supplier"><Input value={company} onChange={(event) => { setCompany(event.target.value); setTested(false); }} placeholder="Supplier company name" /></Field>
        <Field label="Connection type">
          <ChoicePicker
            value={source}
            label="Connection type"
            placeholder="Choose a connection"
            options={[
              { value: "alo", label: "alo invitation link" },
              { value: "api", label: "External pricing API" },
              { value: "file", label: "Spreadsheet or feed" },
            ]}
            onChange={(value) => { setSource(value); setTested(false); }}
          />
        </Field>
      </div>
      {source === "alo" ? (
        <Field label="Invitation link" hint="Your supplier creates this from Shared by me in their alo workspace.">
          <Input value={address} onChange={(event) => { setAddress(event.target.value); setTested(false); }} placeholder="Paste the alo invitation link" />
        </Field>
      ) : (
        <div className="grid grid-cols-2 gap-5 max-md:grid-cols-1">
          <Field label={source === "api" ? "Price API address" : "Feed address"} hint="We detect JSON, XML, CSV and Excel automatically.">
            <Input value={address} onChange={(event) => { setAddress(event.target.value); setTested(false); }} placeholder="https://supplier.example/prices" />
          </Field>
          <Field label="Access key" hint="Kept private and never shown in your customer documents.">
            <Input type="password" value={accessKey} onChange={(event) => { setAccessKey(event.target.value); setTested(false); }} placeholder="Paste the key from your supplier" />
          </Field>
        </div>
      )}
      {tested ? (
        <div className="rounded-xl border border-success/20 bg-success-tint p-4">
          <div className="flex items-center gap-2 text-sm font-semibold text-success"><Check className="size-4" />Connection ready</div>
          <p className="mb-0 mt-1 text-sm text-secondary">148 products found · 131 matched automatically · 17 can be reviewed after connecting.</p>
        </div>
      ) : (
        <Button variant="ghost" icon={<RefreshCw />} disabled={!canTest} onClick={() => setTested(true)}>Test and preview</Button>
      )}
      <details className="group rounded-xl border border-default bg-surface p-4">
        <summary className="flex cursor-pointer list-none items-center gap-2 text-sm font-semibold text-primary">
          <ChevronDown className="size-4 transition-transform group-open:rotate-180" />Advanced settings
        </summary>
        <div className="mt-4 grid gap-4 border-t border-subtle pt-4">
          <section>
            <h3 className="m-0 text-sm font-semibold text-primary">Synchronization and approvals</h3>
            <p className="mb-0 mt-1 text-xs leading-relaxed text-tertiary">Choose when prices are checked and which changes need a person to approve them.</p>
            <div className="mt-3 grid grid-cols-2 gap-4 max-md:grid-cols-1">
              <Field label="Check for updates">
                <ChoicePicker
                  value={schedule}
                  label="Check for updates"
                  placeholder="Choose a schedule"
                  options={[
                    { value: "hourly", label: "Every hour" },
                    { value: "daily", label: "Once a day" },
                    { value: "weekly", label: "Once a week" },
                    { value: "manual", label: "Only when I sync" },
                  ]}
                  onChange={setSchedule}
                />
              </Field>
              <Field label="Apply price changes">
                <ChoicePicker
                  value={changePolicy}
                  label="Apply price changes"
                  placeholder="Choose an approval rule"
                  options={[
                    { value: "review", label: "Review every change" },
                    { value: "limited", label: "Automatic within a limit" },
                    { value: "automatic", label: "Apply all automatically" },
                  ]}
                  onChange={setChangePolicy}
                />
              </Field>
              {changePolicy === "limited" && (
                <Field label="Automatic change limit" hint="Changes above this percentage wait for approval.">
                  <div className="relative"><Input value={changeLimit} onChange={(event) => setChangeLimit(event.target.value)} inputMode="decimal" aria-label="Automatic change limit" className="!pr-10" /><span className="pointer-events-none absolute right-4 top-1/2 -translate-y-1/2 text-sm text-tertiary">%</span></div>
                </Field>
              )}
            </div>
          </section>

          <section className="border-t border-subtle pt-4">
            <h3 className="m-0 text-sm font-semibold text-primary">Product matching</h3>
            <p className="mb-0 mt-1 text-xs leading-relaxed text-tertiary">Control how supplier products are paired with items already in your catalogue.</p>
            <div className="mt-3 grid grid-cols-2 gap-4 max-md:grid-cols-1">
              <Field label="Match products by">
                <ChoicePicker
                  value={matching}
                  label="Match products by"
                  placeholder="Choose matching"
                  options={[
                    { value: "sku", label: "SKU, then barcode and name" },
                    { value: "barcode", label: "Barcode, then SKU and name" },
                    { value: "name", label: "Product name" },
                    { value: "manual", label: "Review every match" },
                  ]}
                  onChange={setMatching}
                />
              </Field>
              <Field label="New supplier products">
                <ChoicePicker
                  value={newProducts}
                  label="New supplier products"
                  placeholder="Choose an action"
                  options={[
                    { value: "review", label: "Hold for review" },
                    { value: "create", label: "Create as draft items" },
                    { value: "ignore", label: "Do not import" },
                  ]}
                  onChange={setNewProducts}
                />
              </Field>
            </div>
          </section>

          {source !== "alo" && (
            <section className="border-t border-subtle pt-4">
              <h3 className="m-0 text-sm font-semibold text-primary">Supplier field mapping</h3>
              <p className="mb-0 mt-1 text-xs leading-relaxed text-tertiary">State the field names used by this supplier. alo will suggest these after the first preview.</p>
              <div className="mt-3 grid grid-cols-4 gap-3 max-lg:grid-cols-2 max-sm:grid-cols-1">
                <Field label="SKU field"><Input value={skuField} onChange={(event) => setSkuField(event.target.value)} /></Field>
                <Field label="Name field"><Input value={nameField} onChange={(event) => setNameField(event.target.value)} /></Field>
                <Field label="Net price field"><Input value={priceField} onChange={(event) => setPriceField(event.target.value)} /></Field>
                <Field label="Currency field"><Input value={currencyField} onChange={(event) => setCurrencyField(event.target.value)} /></Field>
              </div>
            </section>
          )}

          {source === "api" && (
            <section className="border-t border-subtle pt-4">
              <h3 className="m-0 text-sm font-semibold text-primary">Custom authentication header</h3>
              <p className="mb-0 mt-1 text-xs leading-relaxed text-tertiary">Optional. Use this only when the supplier’s documentation requires a header other than the access key above.</p>
              <div className="mt-3 grid grid-cols-2 gap-4 max-md:grid-cols-1">
                <Field label="Header name"><Input value={headerName} onChange={(event) => setHeaderName(event.target.value)} placeholder="X-API-Key" /></Field>
                <Field label="Header value"><Input type="password" value={headerValue} onChange={(event) => setHeaderValue(event.target.value)} placeholder="Enter the secure value" /></Field>
              </div>
            </section>
          )}
        </div>
      </details>
    </Modal>
  );
}

function SharePricesDialog({
  onClose,
  onShared,
  products,
  productsLoading,
}: {
  onClose: () => void;
  onShared: (connection: Connection) => void;
  products: BillingProduct[];
  productsLoading: boolean;
}) {
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
      catalogue: catalogue === "all" ? "Live price list · Automatically updated" : "Selected price-list items",
      items: sharedProducts.length,
      health: "connected",
      detail: delivery === "alo" ? "Waiting for the client to accept in alo" : "External API access is ready to share",
      updated: "Created just now",
      cadence: delivery === "alo" ? "On approval" : "Live",
      channel: delivery === "alo" ? "alo" : "api",
    });
  }

  return (
    <Modal
      title="Share my prices"
      onClose={onClose}
      wide
      icon={<Share2 className="size-5" />}
      actions={<CloseButton onClick={onClose} />}
      footer={
        <div className="ml-auto flex gap-3">
          <Button variant="ghost" onClick={onClose}>{created ? "Close" : "Cancel"}</Button>
          {!created && <Button disabled={!canCreate} onClick={() => setCreated(true)}>Create secure connection</Button>}
          {created && <Button onClick={finish}>Done</Button>}
        </div>
      }
    >
      {!created ? (
        <>
          <div className="rounded-xl border border-accent/20 bg-accent-soft p-4">
            <p className="m-0 text-sm font-semibold text-primary">You control exactly what this client receives</p>
            <p className="mb-0 mt-1 text-sm leading-relaxed text-secondary">Internal purchase costs, supplier names and margins are never included.</p>
          </div>
          <div className="grid grid-cols-2 gap-5 max-md:grid-cols-1">
            <Field label="Client or partner"><Input value={company} onChange={(event) => setCompany(event.target.value)} placeholder="Company name" /></Field>
            <Field label="How will they connect?">
              <ChoicePicker
                value={delivery}
                label="How will they connect?"
                placeholder="Choose delivery"
                options={[
                  { value: "alo", label: "Invite their alo workspace" },
                  { value: "api", label: "Give them external API access" },
                ]}
                onChange={setDelivery}
              />
            </Field>
          </div>
          <Field label="Prices to share">
            <ChoicePicker
              value={catalogue}
              label="Prices to share"
              placeholder="Choose prices"
              options={[
                { value: "all", label: `Live price list · ${products.length} active products` },
                { value: "selected", label: `Choose products · ${selectedProductIds.size} selected` },
              ]}
              onChange={setCatalogue}
            />
          </Field>
          {catalogue === "selected" && (
            <section className="rounded-xl border border-default bg-raised/30 p-3" aria-label="Choose price-list products">
              <div className="relative">
                <Search className="pointer-events-none absolute left-3.5 top-1/2 size-4 -translate-y-1/2 text-tertiary" aria-hidden="true" />
                <Input className="!pl-10" value={productSearch} onChange={(event) => setProductSearch(event.target.value)} placeholder="Search your price list" aria-label="Search your price list" />
              </div>
              <div className="mt-3 grid max-h-52 grid-cols-2 gap-2 overflow-y-auto pr-1 max-md:grid-cols-1">
                {visibleProducts.map((product) => {
                  const selected = selectedProductIds.has(product.id);
                  return (
                    <button
                      key={product.id}
                      type="button"
                      aria-pressed={selected}
                      className={cx(
                        "flex min-h-11 items-center gap-3 rounded-lg border px-3 py-2 text-left text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/30",
                        selected ? "border-accent/30 !bg-accent-soft !text-accent" : "border-default !bg-surface text-primary hover:!border-accent/30 hover:!bg-accent-soft hover:!text-accent",
                      )}
                      onClick={() => toggleProduct(product.id)}
                    >
                      <span className={cx("inline-flex size-5 shrink-0 items-center justify-center rounded-md border", selected ? "border-accent bg-accent text-on-accent" : "border-strong bg-surface")}>
                        {selected && <Check className="size-3.5" aria-hidden="true" />}
                      </span>
                      <span className="min-w-0 flex-1 truncate font-medium">{product.name}</span>
                      <span className="shrink-0 text-xs text-tertiary">{product.unit || "item"}</span>
                    </button>
                  );
                })}
                {!productsLoading && visibleProducts.length === 0 && <p className="col-span-full m-0 px-2 py-4 text-center text-sm text-secondary">No price-list products match this search.</p>}
                {productsLoading && <p className="col-span-full m-0 px-2 py-4 text-center text-sm text-secondary">Loading your price list…</p>}
              </div>
            </section>
          )}
          <div className="grid grid-cols-3 gap-3 max-md:grid-cols-1">
            {[
              ["Prices", catalogue === "all" ? `Live price list (${products.length})` : `${selectedProductIds.size} selected products`],
              ["Updates", "Price-list changes flow through this connection"],
              ["Validity", "No expiry"],
            ].map(([label, value]) => (
              <div key={label} className="rounded-xl border border-default bg-raised/40 p-4">
                <p className="m-0 text-xs font-semibold uppercase tracking-wide text-tertiary">{label}</p>
                <p className="mb-0 mt-1.5 text-sm font-medium text-primary">{value}</p>
              </div>
            ))}
          </div>
        </>
      ) : (
        <div className="flex flex-col gap-5">
          <div className="flex items-start gap-3 rounded-xl border border-success/20 bg-success-tint p-4">
            <ShieldCheck className="mt-0.5 size-5 shrink-0 text-success" />
            <div><p className="m-0 text-sm font-semibold text-primary">Secure price connection created</p><p className="mb-0 mt-1 text-sm text-secondary">Send this to {company}. Access can be paused or revoked at any time.</p></div>
          </div>
          <Field label={delivery === "alo" ? "alo invitation link" : "Price API address"}>
            <div className="flex gap-2"><Input readOnly value={delivery === "alo" ? invite : "https://api.alo.example/v1/shared-prices/AL7K-Q9M2"} /><Button variant="ghost" icon={<Copy />} onClick={() => void navigator.clipboard?.writeText(delivery === "alo" ? invite : "https://api.alo.example/v1/shared-prices/AL7K-Q9M2")}>Copy</Button></div>
          </Field>
          {delivery === "api" && <Field label="Access key" hint="The full key is shown only when it is created."><div className="flex gap-2"><Input readOnly value={key} /><Button variant="ghost" icon={<Copy />} onClick={() => void navigator.clipboard?.writeText(key)}>Copy</Button></div></Field>}
        </div>
      )}
    </Modal>
  );
}

export function PriceConnectionsView() {
  const { confirm } = useDialogs();
  const api = useBillingApi();
  const [direction, setDirection] = useState<Direction>("received");
  const [connections, setConnections] = useState<Connection[]>(storedConnections);
  const [search, setSearch] = useState("");
  const [dialog, setDialog] = useState<Direction | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [products, setProducts] = useState<BillingProduct[]>([]);
  const [productsLoading, setProductsLoading] = useState(true);
  const shown = useMemo(() => {
    const needle = search.trim().toLocaleLowerCase();
    return connections.filter((connection) => connection.direction === direction && (needle === "" || `${connection.company} ${connection.catalogue}`.toLocaleLowerCase().includes(needle)));
  }, [connections, direction, search]);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(connections));
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

  function add(connection: Connection) {
    setConnections((current) => [connection, ...current]);
    setDirection(connection.direction);
    setDialog(null);
    setNotice(`${connection.company} is now ${connection.direction === "received" ? "supplying prices to" : "receiving prices from"} this workspace.`);
  }

  return (
    <div className="mx-auto flex min-h-0 w-full max-w-[112rem] flex-1 flex-col gap-4 overflow-y-auto px-8 pb-8 pt-6 max-[52rem]:p-4">
      <section className="flex flex-wrap items-center gap-4 px-1 py-1">
        <div className="min-w-0 flex-1"><h2 className="m-0 text-xl font-semibold tracking-tight text-primary">Price connections</h2><p className="mb-0 mt-1 text-sm leading-relaxed text-secondary">Receive live supplier costs and securely share selected selling prices with your clients.</p></div>
        <div className="flex flex-wrap gap-2"><Button variant="ghost" icon={<ArrowDownToLine />} onClick={() => setDialog("received")}>Connect supplier prices</Button><Button icon={<Share2 />} onClick={() => setDialog("shared")}>Share my prices</Button></div>
      </section>

      <section className="flex flex-wrap items-center gap-4 rounded-xl border border-default bg-surface p-3 shadow-sm">
          <div className="inline-flex gap-1" role="tablist" aria-label="Price connection direction">
            {(["received", "shared"] as const).map((tab) => (
              <button key={tab} type="button" role="tab" aria-selected={direction === tab} className={cx("inline-flex min-h-10 items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30", direction === tab ? "bg-accent-soft text-accent" : "text-secondary hover:bg-raised hover:text-primary")} onClick={() => setDirection(tab)}>
                {tab === "received" ? <ArrowDownToLine className="size-4" /> : <ArrowUpFromLine className="size-4" />}
                {tab === "received" ? "Received by me" : "Shared by me"}
              </button>
            ))}
          </div>
          <label className="relative ml-auto flex min-w-64 items-center max-sm:ml-0 max-sm:w-full"><Search className="pointer-events-none absolute left-3.5 size-4 text-tertiary" /><Input type="search" value={search} onChange={(event) => setSearch(event.target.value)} className="!pl-10" placeholder="Search connections" aria-label="Search connections" /></label>
      </section>

      {notice !== null && <div className="flex items-center gap-3 rounded-xl border border-success/20 bg-success-tint px-4 py-3 text-sm text-primary" role="status"><Check className="size-4 shrink-0 text-success" />{notice}<button type="button" className="ml-auto rounded-lg p-2 text-tertiary hover:bg-surface hover:text-primary" aria-label="Dismiss" onClick={() => setNotice(null)}><X className="size-4" /></button></div>}

      <div className="grid gap-4">
        {shown.map((connection) => <ConnectionCard key={connection.id} connection={connection} onSync={() => { setConnections((current) => current.map((item) => item.id === connection.id ? { ...item, updated: "Updated just now", health: "connected" } : item)); setNotice(`${connection.company} is up to date.`); }} onToggle={() => setConnections((current) => current.map((item) => item.id === connection.id ? { ...item, health: item.health === "paused" ? "connected" : "paused" } : item))} onRemove={() => { void (async () => {
          const accepted = await confirm({
            title: "Disconnect price connection?",
            message: `${connection.company} will stop ${connection.direction === "received" ? "sending supplier prices to" : "receiving prices from"} this workspace. Existing prices will be kept, but they will no longer update automatically.`,
            confirmLabel: "Disconnect",
            cancelLabel: "Keep connected",
            danger: true,
          });
          if (accepted) setConnections((current) => current.filter((item) => item.id !== connection.id));
        })(); }} />)}
        {shown.length === 0 && <div className="flex min-h-56 flex-col items-center justify-center rounded-2xl border border-dashed border-default bg-surface p-8 text-center"><Link2 className="size-8 text-accent" /><h3 className="mb-0 mt-3 text-base font-semibold text-primary">No matching connections</h3><p className="mb-0 mt-1 text-sm text-secondary">Try another search or create a new price connection.</p></div>}
      </div>

      {dialog === "received" && <ConnectSupplierDialog onClose={() => setDialog(null)} onConnected={add} />}
      {dialog === "shared" && <SharePricesDialog products={products} productsLoading={productsLoading} onClose={() => setDialog(null)} onShared={add} />}
    </div>
  );
}
