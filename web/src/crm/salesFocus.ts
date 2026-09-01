import type { CrmDeal } from "./types";

const DAY_MS = 86_400_000;
const CLOSING_SOON_DAYS = 14;
const QUIET_DAYS = 14;

export type DealAttention = "overdue" | "quiet" | null;

function dayStart(value: Date): number {
  return new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
}

function expectedCloseTime(deal: CrmDeal): number | null {
  if (deal.expectedClose === null) return null;
  const value = new Date(`${deal.expectedClose}T00:00:00`);
  const time = value.getTime();
  return Number.isFinite(time) ? time : null;
}

export function dealAttention(deal: CrmDeal, now: Date): DealAttention {
  if (deal.state !== "open") return null;
  const close = expectedCloseTime(deal);
  if (close !== null && close < dayStart(now)) return "overdue";
  const updated = new Date(deal.updatedAt).getTime();
  if (Number.isFinite(updated) && now.getTime() - updated >= QUIET_DAYS * DAY_MS) return "quiet";
  return null;
}

export function salesFocus(deals: CrmDeal[], now = new Date()) {
  const open = deals.filter((deal) => deal.state === "open");
  const today = dayStart(now);
  const soon = today + CLOSING_SOON_DAYS * DAY_MS;
  const closingSoon = open.filter((deal) => {
    const close = expectedCloseTime(deal);
    return close !== null && close >= today && close <= soon;
  });
  const overdue = open.filter((deal) => dealAttention(deal, now) === "overdue");
  const quiet = open.filter((deal) => dealAttention(deal, now) === "quiet");
  const attention = [...overdue, ...quiet].sort((a, b) => {
    const aClose = expectedCloseTime(a) ?? Number.MAX_SAFE_INTEGER;
    const bClose = expectedCloseTime(b) ?? Number.MAX_SAFE_INTEGER;
    return aClose - bClose || a.updatedAt.localeCompare(b.updatedAt);
  });

  return { open, closingSoon, overdue, quiet, attention };
}
