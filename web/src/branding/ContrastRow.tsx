import { AlertCircle, CheckCircle2 } from "lucide-react";

import { strings } from "../i18n";
import { contrastPasses } from "./colorTools";

export function ContrastRow({ label, color, ink }: { label: string; color: string; ink: string }) {
  const pass = contrastPasses(color, ink);
  return (
    <div className="flex items-center gap-3 rounded-xl bg-raised p-2.5">
      <span className="grid size-10 shrink-0 place-items-center rounded-lg text-sm font-bold" style={{ background: color, color: ink }}>Aa</span>
      <div className="min-w-0 flex-1"><strong className="block truncate text-xs text-primary">{label}</strong><small className="text-[0.68rem] text-tertiary">{ink === "#FFFFFF" ? strings.brandingUseLightText : strings.brandingUseDarkText}</small></div>
      {pass ? <CheckCircle2 className="size-4 text-success" aria-hidden="true" /> : <AlertCircle className="size-4 text-danger" aria-hidden="true" />}
    </div>
  );
}
