import { Check, CircleAlert, Pause } from "lucide-react";

import { strings } from "../i18n";

export type PriceConnectionDirection = "received" | "shared";
export type PriceConnectionHealth = "connected" | "attention" | "paused";

export interface PriceConnection {
  id: string;
  direction: PriceConnectionDirection;
  company: string;
  catalogue: string;
  items: number;
  health: PriceConnectionHealth;
  detail: string;
  updated: string;
  cadence: string;
  changes?: number;
  channel: "alo" | "api";
}

export const PRICE_CONNECTIONS_STORAGE_KEY = "alo.billing.price-connections.v1";

export const INITIAL_PRICE_CONNECTIONS: PriceConnection[] = [
  { id: "received-nordwerk", direction: "received", company: "Nordwerk Components", catalogue: strings.billingConnectionsIndustrialComponentsEur, items: 386, health: "connected", detail: strings.billingConnectionsChangesReady(4), updated: strings.billingConnectionsUpdatedMinutesAgo(12), cadence: strings.billingConnectionsDaily, changes: 4, channel: "alo" },
  { id: "received-rotterdam", direction: "received", company: "Rotterdam Metals BV", catalogue: strings.billingConnectionsMetalsSheetEur, items: 124, health: "attention", detail: strings.billingConnectionsSupplierRenew, updated: strings.billingConnectionsUpdatedDaysAgo(2), cadence: strings.billingConnectionsDaily, channel: "api" },
  { id: "shared-atlas", direction: "shared", company: "Atlas Advisory GmbH", catalogue: strings.billingConnectionsWholesaleContract, items: 82, health: "connected", detail: strings.billingConnectionsWorkspaceReceivesApproved, updated: strings.billingConnectionsUsedHoursAgo(1), cadence: strings.billingConnectionsOnApproval, channel: "alo" },
  { id: "shared-harbor", direction: "shared", company: "Harbor Logistics NV", catalogue: strings.billingConnectionsProjectSupplyEur, items: 24, health: "connected", detail: strings.billingConnectionsApiExpiryDemo, updated: strings.billingConnectionsUsedYesterday, cadence: strings.billingConnectionsLive, channel: "api" },
];

export function loadStoredPriceConnections(): PriceConnection[] {
  try {
    const value = localStorage.getItem(PRICE_CONNECTIONS_STORAGE_KEY);
    if (value === null) return INITIAL_PRICE_CONNECTIONS;
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) ? (parsed as PriceConnection[]) : INITIAL_PRICE_CONNECTIONS;
  } catch {
    return INITIAL_PRICE_CONNECTIONS;
  }
}

const HEALTH_PRESENTATION = {
  connected: { className: "bg-success-tint text-success ring-success/15", Icon: Check },
  attention: { className: "bg-danger-tint text-danger ring-danger/15", Icon: CircleAlert },
  paused: { className: "bg-raised text-secondary ring-default", Icon: Pause },
} as const;

export function getPriceConnectionHealthPresentation(health: PriceConnectionHealth) {
  const labels = {
    connected: strings.billingConnectionsConnected,
    attention: strings.billingConnectionsActionNeeded,
    paused: strings.billingConnectionsPaused,
  };
  return { ...HEALTH_PRESENTATION[health], label: labels[health] };
}
