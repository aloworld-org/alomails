import { Check, Copy } from "lucide-react";
import { useState } from "react";

import { strings } from "../i18n";
import { readableInk, toneScale } from "./colorTools";

export function BrandToneScale({ color }: { color: string }) {
  const [copied, setCopied] = useState<string | null>(null);

  const copy = async (value: string) => {
    await navigator.clipboard?.writeText(value);
    setCopied(value);
  };

  return (
    <div className="mt-4 grid h-14 grid-cols-6 overflow-hidden rounded-xl border border-black/5 shadow-sm">
      {toneScale(color).map((tone) => {
        const ink = readableInk(tone);
        return (
          <button
            key={tone}
            type="button"
            className="group grid min-w-0 place-items-center outline-none transition-transform hover:z-10 hover:scale-105 focus-visible:z-10 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent"
            style={{ backgroundColor: tone, color: ink }}
            aria-label={copied === tone ? strings.brandingColorCopied(tone) : strings.brandingCopyColor(tone)}
            title={tone}
            onClick={() => void copy(tone)}
          >
            {copied === tone ? <Check className="size-4" aria-hidden="true" /> : <Copy className="size-4 opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100" aria-hidden="true" />}
          </button>
        );
      })}
    </div>
  );
}
