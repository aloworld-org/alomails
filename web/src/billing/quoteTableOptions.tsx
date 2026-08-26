import { createContext, useContext } from "react";
import type { ReactNode } from "react";

export type QuoteTableLayout = "compact" | "detailed" | "catalogue";

export interface QuoteLineContent {
  description: string;
  image: string;
}

export interface QuoteTableOptionsValue {
  enabled: boolean;
  layout: QuoteTableLayout;
  showImages: boolean;
  showDescriptions: boolean;
  lineContent: Record<string, QuoteLineContent>;
  updateLineContent: (key: string, patch: Partial<QuoteLineContent>) => void;
}

const QuoteTableOptions = createContext<QuoteTableOptionsValue>({
  enabled: false,
  layout: "compact",
  showImages: false,
  showDescriptions: false,
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
