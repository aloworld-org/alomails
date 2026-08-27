import type React from "react";
import { Check, Table2, X } from "lucide-react";
import { Button, Modal, cx } from "../../ds";
import { strings } from "../../i18n";
import type { QuoteStudioDesign as Design } from "./QuoteStudioDesign";
import { LayoutPreview } from "./LayoutPreview";
import { TableToggle } from "./TableToggle";
import { TotalsPreview } from "./TotalsPreview";
export function CustomizeTable({
  design,
  saveError,
  onChange,
  onClose,
}: {
  design: Design;
  saveError: string;
  onChange: React.Dispatch<React.SetStateAction<Design>>;
  onClose: () => void;
}) {
  return (
    <Modal
      title={strings.quoteStudioTableSettings}
      icon={<Table2 className="size-5" />}
      onClose={onClose}
      wide
      actions={
        <button
          type="button"
          className="flex size-9 items-center justify-center rounded-lg text-tertiary hover:bg-accent-soft hover:text-accent"
          aria-label={strings.quoteStudioCloseTableSettings}
          onClick={onClose}
        >
          <X className="size-4" />
        </button>
      }
      footer={
        <>
          <p
            className={cx(
              "mr-auto text-xs",
              saveError ? "text-danger" : "text-secondary",
            )}
          >
            {saveError || strings.quoteStudioTableChangesSavedAutomatically}
          </p>
          <Button onClick={onClose}>{strings.quoteStudioDone}</Button>
        </>
      }
    >
      <section>
        <h3 className="text-sm font-semibold text-primary">
          {strings.quoteStudioChooseLayout}
        </h3>
        <p className="mt-1 text-sm text-secondary">
          {strings.quoteStudioChooseLayoutHelp}
        </p>
        <div className="mt-5 grid gap-5 sm:grid-cols-3">
          {(
            [
              [
                "compact",
                strings.quoteStudioCompact,
                strings.quoteStudioCompactHelp,
              ],
              [
                "detailed",
                strings.quoteStudioDetailed,
                strings.quoteStudioDetailedHelp,
              ],
              [
                "catalogue",
                strings.quoteStudioCatalogue,
                strings.quoteStudioCatalogueHelp,
              ],
            ] as const
          ).map(([layout, label, help]) => (
            <button
              key={layout}
              type="button"
              className={cx(
                "group relative min-h-64 overflow-hidden rounded-2xl border bg-surface !p-5 text-left ring-1 transition-colors hover:border-accent hover:bg-accent-soft/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                design.tableLayout === layout
                  ? "border-accent bg-accent-soft ring-accent/25"
                  : "border-default ring-default hover:bg-accent-soft/30",
              )}
              onClick={() =>
                onChange((current) => ({
                  ...current,
                  tableLayout: layout,
                  showProductDescriptions: layout !== "compact",
                  showProductImages: layout === "catalogue",
                }))
              }
            >
              <LayoutPreview
                layout={layout}
                selected={design.tableLayout === layout}
              />
              <span className="mt-4 flex items-start justify-between gap-4 px-1 pb-2">
                <span>
                  <strong className="block text-sm font-semibold text-primary">
                    {label}
                  </strong>
                  <span className="mt-1 block text-xs leading-relaxed text-secondary">
                    {help}
                  </span>
                </span>
                <span
                  className={cx(
                    "mt-0.5 grid size-5 shrink-0 place-items-center rounded-full border",
                    design.tableLayout === layout
                      ? "border-accent bg-accent text-on-accent"
                      : "border-default bg-surface group-hover:border-accent",
                  )}
                >
                  {design.tableLayout === layout && (
                    <Check className="size-3.5" />
                  )}
                </span>
              </span>
            </button>
          ))}
        </div>
      </section>

      <section className="mt-8 border-t border-subtle pt-7">
        <h3 className="text-sm font-semibold text-primary">
          {strings.quoteStudioProductContent}
        </h3>
        <p className="mt-1 text-sm text-secondary">
          {strings.quoteStudioProductContentHelp}
        </p>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          <TableToggle
            label={strings.quoteStudioProductImages}
            help={strings.quoteStudioProductImagesHelp}
            checked={design.showProductImages}
            onClick={() =>
              onChange((current) => ({
                ...current,
                showProductImages: !current.showProductImages,
              }))
            }
          />
          <TableToggle
            label={strings.quoteStudioProductDescriptions}
            help={strings.quoteStudioProductDescriptionsHelp}
            checked={design.showProductDescriptions}
            onClick={() =>
              onChange((current) => ({
                ...current,
                showProductDescriptions: !current.showProductDescriptions,
              }))
            }
          />
        </div>
      </section>

      <section className="mt-8 border-t border-subtle pt-7">
        <h3 className="text-sm font-semibold text-primary">
          {strings.quoteStudioVisibleColumns}
        </h3>
        <p className="mt-1 text-sm text-secondary">
          {strings.quoteStudioVisibleColumnsHelp}
        </p>
        <div className="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {(
            [
              ["unit", strings.quoteStudioUnit],
              ["quantity", strings.quoteStudioQuantity],
              ["unitPrice", strings.quoteStudioUnitPrice],
              ["vat", strings.quoteStudioVatRate],
              ["net", strings.quoteStudioLineTotal],
            ] as const
          ).map(([key, label]) => (
            <TableToggle
              key={key}
              label={label}
              help={strings.quoteStudioShowColumn(label)}
              checked={design.columns[key]}
              onClick={() =>
                onChange((current) => ({
                  ...current,
                  columns: { ...current.columns, [key]: !current.columns[key] },
                }))
              }
            />
          ))}
        </div>
      </section>

      <section className="mt-8 border-t border-subtle pt-7">
        <h3 className="text-sm font-semibold text-primary">
          {strings.quoteStudioPricingTableTotals}
        </h3>
        <p className="mt-1 text-sm text-secondary">
          {strings.quoteStudioPricingTableTotalsHelp}
        </p>
        <div className="mt-5 grid gap-5 sm:grid-cols-3">
          {(
            [
              [
                "summary",
                strings.quoteStudioSummaryCard,
                strings.quoteStudioSummaryCardHelp,
              ],
              [
                "full",
                strings.quoteStudioFullWidth,
                strings.quoteStudioFullWidthHelp,
              ],
              [
                "footer",
                strings.quoteStudioTableFooter,
                strings.quoteStudioTableFooterHelp,
              ],
            ] as const
          ).map(([placement, label, help]) => (
            <button
              key={placement}
              type="button"
              className={cx(
                "group min-h-40 rounded-2xl border !p-5 text-left transition-colors hover:border-accent hover:bg-accent-soft/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25",
                design.totalsPlacement === placement
                  ? "border-accent bg-accent-soft ring-1 ring-accent/20"
                  : "border-default bg-surface",
              )}
              onClick={() =>
                onChange((current) => ({
                  ...current,
                  totalsPlacement: placement,
                }))
              }
            >
              <TotalsPreview placement={placement} />
              <span className="mt-4 flex items-start justify-between gap-4 px-1 pb-1">
                <span>
                  <strong className="block text-sm font-semibold text-primary">
                    {label}
                  </strong>
                  <span className="mt-0.5 block text-xs text-secondary">
                    {help}
                  </span>
                </span>
                <span
                  className={cx(
                    "grid size-5 shrink-0 place-items-center rounded-full border",
                    design.totalsPlacement === placement
                      ? "border-accent bg-accent text-on-accent"
                      : "border-default group-hover:border-accent",
                  )}
                >
                  {design.totalsPlacement === placement && (
                    <Check className="size-3.5" />
                  )}
                </span>
              </span>
            </button>
          ))}
        </div>

        <h4 className="mt-10 border-t border-subtle pt-7 text-xs font-semibold uppercase tracking-wide text-tertiary">
          {strings.quoteStudioAmountDetails}
        </h4>
        <div className="mt-5 grid gap-4 sm:grid-cols-3">
          {(
            [
              [
                "total",
                strings.quoteStudioTotalOnly,
                strings.quoteStudioTotalOnlyHelp,
              ],
              [
                "summary",
                strings.quoteStudioNetVatTotal,
                strings.quoteStudioNetVatTotalHelp,
              ],
              [
                "breakdown",
                strings.quoteStudioVatBreakdown,
                strings.quoteStudioVatBreakdownHelp,
              ],
            ] as const
          ).map(([detail, label, help]) => (
            <TableToggle
              key={detail}
              label={label}
              help={help}
              checked={design.totalsDetail === detail}
              onClick={() =>
                onChange((current) => ({ ...current, totalsDetail: detail }))
              }
            />
          ))}
        </div>
        <div className="mt-4 grid gap-4 sm:grid-cols-3">
          <TableToggle
            label={strings.quoteStudioCurrencyCode}
            help={strings.quoteStudioCurrencyCodeHelp}
            checked={design.showCurrencyCode}
            onClick={() =>
              onChange((current) => ({
                ...current,
                showCurrencyCode: !current.showCurrencyCode,
              }))
            }
          />
          <TableToggle
            label={strings.quoteStudioEmphasizeTotal}
            help={strings.quoteStudioEmphasizeTotalHelp}
            checked={design.emphasizeTotal}
            onClick={() =>
              onChange((current) => ({
                ...current,
                emphasizeTotal: !current.emphasizeTotal,
              }))
            }
          />
          <TableToggle
            label={strings.quoteStudioVatNote}
            help={strings.quoteStudioVatNoteHelp}
            checked={design.showTaxNote}
            onClick={() =>
              onChange((current) => ({
                ...current,
                showTaxNote: !current.showTaxNote,
              }))
            }
          />
        </div>
      </section>
    </Modal>
  );
}
