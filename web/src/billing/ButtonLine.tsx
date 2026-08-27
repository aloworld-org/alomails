import { Plus } from "lucide-react";
import { strings } from "../i18n";

export function ButtonLine({ onClick }: { onClick: () => void }) {
  return <button type="button" className="inline-flex min-h-10 items-center gap-2 rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-on-accent transition-colors hover:bg-accent-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" onClick={onClick}><Plus className="size-4" aria-hidden="true" />{strings.billingAddLine}</button>;
}
