import { Check, CircleAlert, ClockAlert, Pause } from "lucide-react";

import { strings } from "../i18n";

export type PriceConnectionDirection = "received" | "shared";
export type PriceConnectionHealth = "connected" | "attention" | "paused" | "expired";
export type PriceConnectionCadence = "hourly" | "daily" | "weekly" | "manual" | "live" | "approval";

export interface PriceConnection {
  id: string;
  direction: PriceConnectionDirection;
  company: string;
  catalogue: string;
  itemCount: number;
  health: PriceConnectionHealth;
  cadence: PriceConnectionCadence;
  channel: "alo" | "api";
  changesCount: number;
  lastSyncedAt: string | null;
  expiresAt: string | null;
  productIds: string[];
  createdAt: string;
  updatedAt: string;
}

export interface PriceConnectionDraft {
  direction: PriceConnectionDirection;
  company: string;
  catalogue: string;
  cadence: PriceConnectionCadence;
  channel: "alo" | "api";
  productIds: string[];
}

const HEALTH_PRESENTATION = {
  connected: { className: "bg-success-tint text-success ring-success/15", Icon: Check },
  attention: { className: "bg-danger-tint text-danger ring-danger/15", Icon: CircleAlert },
  paused: { className: "bg-raised text-secondary ring-default", Icon: Pause },
  expired: { className: "bg-danger-tint text-danger ring-danger/15", Icon: ClockAlert },
} as const;

export function getPriceConnectionHealthPresentation(health: PriceConnectionHealth) {
  const labels = {
    connected: strings.billingConnectionsConnected,
    attention: strings.billingConnectionsActionNeeded,
    paused: strings.billingConnectionsPaused,
    expired: strings.billingConnectionsExpired,
  };
  return { ...HEALTH_PRESENTATION[health], label: labels[health] };
}

export function getPriceConnectionCadenceLabel(cadence: PriceConnectionCadence): string {
  return {
    hourly: strings.billingConnectionsHourly,
    daily: strings.billingConnectionsDaily,
    weekly: strings.billingConnectionsWeekly,
    manual: strings.billingConnectionsManual,
    live: strings.billingConnectionsLive,
    approval: strings.billingConnectionsOnApproval,
  }[cadence];
}
