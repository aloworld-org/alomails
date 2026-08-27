import { strings } from "../../i18n";

export function EmptyBuilder({ readOnly }: { readOnly: boolean }) {
  return (
    <div className="flex min-h-28 items-center justify-center rounded-xl border border-dashed border-default bg-[var(--quote-background)] px-6 py-8 text-center">
      <div>
        <h3 className="text-base font-semibold text-primary">
          {readOnly
            ? strings.quoteStudioNoProposalContent
            : strings.quoteStudioStartQuotationBelow}
        </h3>
        {!readOnly && (
          <p className="mt-1 text-sm text-secondary">
            {strings.quoteStudioFirstBlockHelp}
          </p>
        )}
      </div>
    </div>
  );
}
