import { strings } from "../i18n";
import type { CreationTemplate } from "./DocumentEditor";
import { blankRow, rowFromProduct } from "./lineRows";
import type { BillingProduct } from "./types";

export function quoteCreationTemplates(
  products: BillingProduct[],
): CreationTemplate[] {
  const services = products.filter(
    (product) => !product.stocked && !product.archived,
  );
  const rowsFromProducts =
    (selected: typeof services) => (nextKey: () => string) =>
      selected.map((product) =>
        rowFromProduct({ ...blankRow(nextKey()), qty: "1" }, product),
      );
  const monthly = services.find(
    (product) => product.unit.toLowerCase() === "month",
  );

  return [
    {
      key: "blank",
      name: strings.billingQuoteTemplateBlank,
      description: strings.billingQuoteTemplateBlankDescription,
      preview: "blank",
      buildRows: () => [],
    },
    {
      key: "services",
      name: strings.billingQuoteTemplateServices,
      description: strings.billingQuoteTemplateServicesDescription,
      preview: "services",
      buildRows: rowsFromProducts(services.slice(0, 2)),
    },
    {
      key: "project",
      name: strings.billingQuoteTemplateProject,
      description: strings.billingQuoteTemplateProjectDescription,
      preview: "project",
      buildRows: rowsFromProducts(services.slice(0, 3)),
    },
    {
      key: "retainer",
      name: strings.billingQuoteTemplateRetainer,
      description: strings.billingQuoteTemplateRetainerDescription,
      preview: "retainer",
      buildRows: rowsFromProducts(
        monthly === undefined ? services.slice(0, 1) : [monthly],
      ),
    },
  ];
}
