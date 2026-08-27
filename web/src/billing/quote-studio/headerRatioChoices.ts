import type { HeaderRatio } from "./QuoteStudioDesign";

export interface HeaderRatioChoice {
  id: HeaderRatio;
  columns: string;
  reverseColumns: string;
}

export const HEADER_RATIO_CHOICES: HeaderRatioChoice[] = [
  {
    id: "40-60",
    columns: "grid-cols-[2fr_3fr]",
    reverseColumns: "grid-cols-[3fr_2fr]",
  },
  { id: "50-50", columns: "grid-cols-2", reverseColumns: "grid-cols-2" },
  {
    id: "60-40",
    columns: "grid-cols-[3fr_2fr]",
    reverseColumns: "grid-cols-[2fr_3fr]",
  },
];
