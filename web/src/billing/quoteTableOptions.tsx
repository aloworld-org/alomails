import { createContext, useContext } from "react";
import type { ReactNode } from "react";

export type QuoteTableLayout = "compact" | "detailed" | "catalogue";
export type QuoteTotalsPlacement = "summary" | "full" | "footer";
export type QuoteTotalsDetail = "total" | "summary" | "breakdown";

export interface QuoteLineContent {
  description: string;
  image: string;
  imageFit?: "cover" | "contain";
  imagePosition?: "center" | "top" | "bottom" | "left" | "right";
  imageZoom?: 100 | 125 | 150;
}

export interface QuoteTableOptionsValue {
  enabled: boolean;
  layout: QuoteTableLayout;
  showImages: boolean;
  showDescriptions: boolean;
  totalsPlacement: QuoteTotalsPlacement;
  totalsDetail: QuoteTotalsDetail;
  showCurrencyCode: boolean;
  emphasizeTotal: boolean;
  showTaxNote: boolean;
  lineContent: Record<string, QuoteLineContent>;
  updateLineContent: (key: string, patch: Partial<QuoteLineContent>) => void;
}

const QuoteTableOptions = createContext<QuoteTableOptionsValue>({
  enabled: false,
  layout: "compact",
  showImages: false,
  showDescriptions: false,
  totalsPlacement: "summary",
  totalsDetail: "summary",
  showCurrencyCode: false,
  emphasizeTotal: true,
  showTaxNote: false,
  lineContent: {},
  updateLineContent: () => undefined,
});

export function QuoteTableOptionsProvider({
  value,
  children,
}: {
  value: QuoteTableOptionsValue;
  children: ReactNode;
}) {
  return (
    <QuoteTableOptions.Provider value={value}>
      {children}
    </QuoteTableOptions.Provider>
  );
}

export function useQuoteTableOptions() {
  return useContext(QuoteTableOptions);
}
