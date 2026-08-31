import { Eye, FileDown, Palette, Pencil } from "lucide-react";

import { strings } from "../i18n";
import styles from "./billingStyles";

type Props = {
  draft: boolean;
  preview: boolean;
  downloading: boolean;
  onEdit: () => void;
  onCustomize: () => void;
  onTogglePreview: () => void;
  onDownloadPdf: () => void;
};

const activeClassName =
  "inline-flex min-h-10 items-center gap-2 rounded-xl bg-accent-soft px-4 py-2 text-sm font-medium text-accent no-underline transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25";

export function InvoiceEditorToolbar({
  draft,
  preview,
  downloading,
  onEdit,
  onCustomize,
  onTogglePreview,
  onDownloadPdf,
}: Props) {
  return (
    <div className="flex items-center gap-2">
      {draft && (
        <>
          <button
            type="button"
            className={!preview ? activeClassName : styles.linkAction}
            onClick={onEdit}
            aria-pressed={!preview}
          >
            <Pencil size={15} aria-hidden="true" />
            {strings.billingInvoiceEdit}
          </button>
          <button
            type="button"
            className={styles.linkAction}
            onClick={onCustomize}
          >
            <Palette size={15} aria-hidden="true" />
            {strings.billingCustomizeInvoice}
          </button>
        </>
      )}
      <button
        type="button"
        className={preview ? activeClassName : styles.linkAction}
        aria-pressed={preview}
        onClick={onTogglePreview}
      >
        <Eye size={15} aria-hidden="true" />
        {preview ? strings.billingExitPreview : strings.billingInvoicePreview}
      </button>
      <button
        type="button"
        className={styles.linkAction}
        onClick={onDownloadPdf}
        disabled={downloading}
        aria-busy={downloading}
      >
        <FileDown size={15} aria-hidden="true" />
        {strings.billingDownloadPdf}
      </button>
    </div>
  );
}
