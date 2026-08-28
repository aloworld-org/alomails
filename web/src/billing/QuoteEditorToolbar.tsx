import { Eye, FileDown, Palette, Pencil } from "lucide-react";

import { strings } from "../i18n";
import styles from "./billingStyles";

type Props = {
  creatingRevision: boolean;
  draft: boolean;
  preview: boolean;
  /** Fetching the PDF right now; the button waits rather than double-firing. */
  downloading: boolean;
  onCustomize: () => void;
  onEdit: () => void;
  onTogglePreview: () => void;
  /** Saves the offer as the PDF the customer receives — `undefined` until the
   *  offer exists on the server. */
  onDownloadPdf: (() => void) | undefined;
};

const activeClassName =
  "inline-flex min-h-10 items-center gap-2 rounded-xl bg-accent-soft px-4 py-2 text-sm font-medium text-accent no-underline transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25";

export function QuoteEditorToolbar({
  creatingRevision,
  draft,
  preview,
  downloading,
  onCustomize,
  onEdit,
  onTogglePreview,
  onDownloadPdf,
}: Props) {
  return (
    <div className="flex items-center gap-2">
      <button
        type="button"
        className={draft && !preview ? activeClassName : styles.linkAction}
        onClick={onEdit}
        disabled={creatingRevision}
        aria-pressed={draft && !preview}
        title={
          preview
            ? strings.billingQuoteExitPreviewToEdit
            : draft
              ? strings.billingQuoteEditContent
              : strings.billingQuoteCreateRevision
        }
      >
        <Pencil size={15} aria-hidden="true" />
        {draft
          ? strings.billingQuoteEdit
          : strings.billingQuoteCreateRevisionAction}
      </button>
      <button
        type="button"
        className={styles.linkAction}
        onClick={onCustomize}
        disabled={creatingRevision}
        title={
          preview
            ? strings.billingQuoteExitPreviewToCustomize
            : !draft
              ? strings.billingQuoteCreateRevisionToCustomize
              : strings.quoteStudioCustomizeQuotation
        }
      >
        <Palette size={15} aria-hidden="true" />
        {strings.quoteStudioCustomizeQuotation}
      </button>
      <button
        type="button"
        className={preview ? activeClassName : styles.linkAction}
        aria-pressed={preview}
        onClick={onTogglePreview}
      >
        <Eye size={15} aria-hidden="true" />
        {preview
          ? strings.billingExitPreview
          : strings.billingQuotationPreview}
      </button>
      <button
        type="button"
        className={styles.linkAction}
        onClick={onDownloadPdf}
        disabled={onDownloadPdf === undefined || downloading}
        aria-busy={downloading}
      >
        <FileDown size={15} aria-hidden="true" />
        {strings.billingDownloadPdf}
      </button>
    </div>
  );
}
