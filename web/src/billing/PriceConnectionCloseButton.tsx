import { X } from "lucide-react";

import { strings } from "../i18n";

export function PriceConnectionCloseButton({ onClick }: { onClick: () => void }) {
  return <button type="button" className="inline-flex size-9 items-center justify-center rounded-lg text-tertiary transition-colors hover:bg-raised hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/30" aria-label={strings.close} onClick={onClick}><X className="size-4" aria-hidden="true" /></button>;
}
