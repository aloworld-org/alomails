import { strings } from "../i18n";
import type { Site } from "./types";

export function SiteStatusChip({ status }: { status: Site["status"] }) {
  const live = status === "live";
  return (
    <span
      className={
        live
          ? "inline-flex items-center gap-1.5 rounded-full bg-success-tint px-2.5 py-1 text-xs font-medium text-success"
          : "inline-flex items-center gap-1.5 rounded-full bg-raised px-2.5 py-1 text-xs font-medium text-secondary"
      }
    >
      <span
        className={
          live
            ? "size-1.5 rounded-full bg-success"
            : "size-1.5 rounded-full bg-tertiary"
        }
        aria-hidden="true"
      />
      {live ? strings.sitesStatusLive : strings.sitesStatusDraft}
    </span>
  );
}
