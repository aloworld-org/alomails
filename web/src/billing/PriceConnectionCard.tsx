import { ArrowDownToLine, ArrowUpFromLine, Clock3, Pause, Play, RefreshCw, Unplug } from "lucide-react";

import { Button, cx } from "../ds";
import { strings } from "../i18n";
import { getPriceConnectionHealthPresentation, type PriceConnection } from "./priceConnectionsModel";

export function PriceConnectionCard({ connection, onSync, onToggle, onRemove }: { connection: PriceConnection; onSync: () => void; onToggle: () => void; onRemove: () => void }) {
  const status = getPriceConnectionHealthPresentation(connection.health);
  return <article className="group flex items-center gap-4 rounded-2xl border border-default bg-surface px-5 py-4 shadow-sm transition-colors hover:border-accent/30 max-md:flex-wrap">
    <span className="inline-flex size-10 shrink-0 items-center justify-center rounded-lg bg-accent-soft text-accent">{connection.direction === "received" ? <ArrowDownToLine className="size-5" aria-hidden="true" /> : <ArrowUpFromLine className="size-5" aria-hidden="true" />}</span>
    <div className="min-w-0 flex-1">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1"><h3 className="m-0 text-base font-semibold text-primary transition-colors group-hover:text-accent">{connection.company}</h3><span className={cx("inline-flex items-center gap-1.5 text-xs font-semibold", connection.health === "connected" ? "text-success" : connection.health === "attention" ? "text-danger" : "text-secondary")}><status.Icon className="size-3.5" aria-hidden="true" />{status.label}</span></div>
      <p className="mb-0 mt-1 text-sm text-secondary">{connection.catalogue}</p>
      <p className="mb-0 mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-tertiary"><span>{strings.billingConnectionsProductCount(connection.items)}</span><span>{strings.billingConnectionsUpdateCadence(connection.cadence)}</span><span>{connection.channel === "alo" ? strings.billingConnectionsViaAlo : strings.billingConnectionsExternalApi}</span><span className="inline-flex items-center gap-1.5"><Clock3 className="size-3" aria-hidden="true" />{connection.updated}</span></p>
      {connection.health === "attention" && <p className="mb-0 mt-2 text-xs font-medium text-danger">{connection.detail}</p>}
    </div>
    <div className="ml-auto flex shrink-0 items-center gap-2 max-md:ml-14">
      {connection.changes !== undefined && connection.changes > 0 && <Button size="sm">{strings.billingConnectionsReviewChanges(connection.changes)}</Button>}
      {connection.direction === "received" && <button type="button" className="inline-flex size-10 items-center justify-center rounded-lg text-secondary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30" aria-label={strings.billingConnectionsSyncNow} title={strings.billingConnectionsSyncNow} onClick={onSync}><RefreshCw className="size-4" aria-hidden="true" /></button>}
      <button type="button" className="inline-flex size-10 items-center justify-center rounded-lg text-secondary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30" aria-label={connection.health === "paused" ? strings.billingConnectionsResume : strings.billingConnectionsPause} title={connection.health === "paused" ? strings.billingConnectionsResume : strings.billingConnectionsPause} onClick={onToggle}>{connection.health === "paused" ? <Play className="size-4" aria-hidden="true" /> : <Pause className="size-4" aria-hidden="true" />}</button>
      <button type="button" className="inline-flex size-10 items-center justify-center rounded-lg text-tertiary transition-colors hover:bg-danger-tint hover:text-danger focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-danger/25" aria-label={strings.billingConnectionsDisconnectCompany(connection.company)} onClick={onRemove}><Unplug className="size-4" aria-hidden="true" /></button>
    </div>
  </article>;
}
