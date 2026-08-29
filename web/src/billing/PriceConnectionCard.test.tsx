import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import { PriceConnectionCard } from "./PriceConnectionCard";

it("renders persisted connection health and locale-aware update metadata", () => {
  render(<PriceConnectionCard connection={{ id: "connection-1", direction: "received", company: "Fictional Supplier", catalogue: "Spring catalogue", itemCount: 100, health: "expired", cadence: "weekly", channel: "api", changesCount: 0, lastSyncedAt: "2026-08-20T10:00:00Z", expiresAt: "2026-08-21T10:00:00Z", productIds: [], createdAt: "2026-08-01T10:00:00Z", updatedAt: "2026-08-20T10:00:00Z" }} onSync={vi.fn()} onToggle={vi.fn()} onRemove={vi.fn()} />);
  expect(screen.getByText("Expired")).toBeTruthy();
  expect(screen.getByText("100 products")).toBeTruthy();
  expect(screen.getByText("Weekly updates")).toBeTruthy();
});
