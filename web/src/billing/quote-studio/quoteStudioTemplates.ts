import { strings } from "../../i18n";
import type { QuoteStudioDesign } from "./QuoteStudioDesign";
import {
  EMPTY_QUOTE_STUDIO_DESIGN,
  normalizeSavedQuoteDesign,
} from "./quoteStudioNormalization";

export type QuoteTemplatePreset =
  | "blank"
  | "services"
  | "project"
  | "retainer";

function templateBlockId(kind: string) {
  return `${kind}-${crypto.randomUUID()}`;
}

export function createQuoteTemplateDesign(
  preset: QuoteTemplatePreset,
): QuoteStudioDesign {
  if (preset === "blank")
    return normalizeSavedQuoteDesign(EMPTY_QUOTE_STUDIO_DESIGN);

  if (preset === "services") {
    return normalizeSavedQuoteDesign({
      ...EMPTY_QUOTE_STUDIO_DESIGN,
      headerStyle: "minimal",
      theme: "minimal",
      tableLayout: "compact",
      totalsPlacement: "footer",
      blocks: [
        {
          id: templateBlockId("heading"),
          kind: "heading",
          level: 1,
          text: strings.quoteStudioTemplateServicesHeading,
        },
        {
          id: templateBlockId("paragraph"),
          kind: "paragraph",
          text: strings.quoteStudioTemplateServicesIntroduction,
        },
        {
          id: templateBlockId("pricing"),
          kind: "pricing",
          title: strings.quoteStudioTemplateServicesTable,
          showSubtotal: true,
        },
      ],
    });
  }

  if (preset === "project") {
    return normalizeSavedQuoteDesign({
      ...EMPTY_QUOTE_STUDIO_DESIGN,
      headerStyle: "band",
      headerRatio: "60-40",
      theme: "editorial",
      tableLayout: "detailed",
      showProductDescriptions: true,
      totalsPlacement: "full",
      blocks: [
        {
          id: templateBlockId("heading"),
          kind: "heading",
          level: 1,
          text: strings.quoteStudioTemplateProjectHeading,
        },
        {
          id: templateBlockId("paragraph"),
          kind: "paragraph",
          text: strings.quoteStudioTemplateProjectIntroduction,
        },
        {
          id: templateBlockId("list"),
          kind: "list",
          ordered: false,
          columns: 3,
          items: [
            strings.quoteStudioTemplateProjectDiscovery,
            strings.quoteStudioTemplateProjectDelivery,
            strings.quoteStudioTemplateProjectHandover,
          ].join("\n"),
        },
        {
          id: templateBlockId("pricing"),
          kind: "pricing",
          title: strings.quoteStudioTemplateProjectTable,
          showSubtotal: true,
        },
      ],
    });
  }

  return normalizeSavedQuoteDesign({
    ...EMPTY_QUOTE_STUDIO_DESIGN,
    headerStyle: "stacked",
    headerRatio: "40-60",
    theme: "modern",
    tableLayout: "detailed",
    totalsPlacement: "summary",
    showCurrencyCode: true,
    blocks: [
      {
        id: templateBlockId("heading"),
        kind: "heading",
        level: 1,
        text: strings.quoteStudioTemplateRetainerHeading,
      },
      {
        id: templateBlockId("paragraph"),
        kind: "paragraph",
        text: strings.quoteStudioTemplateRetainerIntroduction,
      },
      {
        id: templateBlockId("pricing"),
        kind: "pricing",
        title: strings.quoteStudioTemplateRetainerTable,
        showSubtotal: true,
      },
      { id: templateBlockId("divider"), kind: "divider" },
      {
        id: templateBlockId("list"),
        kind: "list",
        ordered: false,
        columns: 2,
        items: [
          strings.quoteStudioTemplateRetainerReporting,
          strings.quoteStudioTemplateRetainerSupport,
        ].join("\n"),
      },
    ],
  });
}
