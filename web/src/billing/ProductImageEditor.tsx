import { Crop, X } from "lucide-react";

import { Button, IconButton, Modal, cx } from "../ds";
import { strings } from "../i18n";
import { EditorChoice } from "./EditorChoice";
import { imageClasses, normalizeZoom, type ImageDraft } from "./productImage";

export function ProductImageEditor({
  draft,
  onChange,
  onApply,
  onClose,
}: {
  draft: ImageDraft;
  onChange: (draft: ImageDraft) => void;
  onApply: () => void;
  onClose: () => void;
}) {
  return (
    <Modal
      title={strings.billingEditProductImage}
      icon={<Crop className="size-5" />}
      onClose={onClose}
      wide
      actions={<IconButton label={strings.billingCloseImageEditor} icon={<X />} onClick={onClose} />}
      footer={<div className="ml-auto flex items-center gap-3"><Button variant="ghost" onClick={onClose}>{strings.cancel}</Button><Button onClick={onApply}>{strings.billingApplyImage}</Button></div>}
    >
      <div className="grid min-h-0 gap-5 lg:grid-cols-[minmax(0,1fr)_15rem]">
        <section className="rounded-xl border border-default bg-app p-5" aria-label={strings.billingPdfPreview}>
          <div className="mx-auto max-w-xl rounded-lg border border-default bg-surface p-8 shadow-sm">
            <div className="mb-5 flex items-center justify-between border-b border-subtle pb-4">
              <div><span className="text-xs font-semibold uppercase tracking-wide text-accent">{strings.billingQuotationPreview}</span><p className="mt-1 text-sm text-secondary">{strings.billingImagePdfHelp}</p></div>
              <span className="text-xs font-medium text-tertiary">{strings.billingPdfPaperSizeA4}</span>
            </div>
            <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-4 rounded-lg border border-subtle p-4">
              <div className="size-24 overflow-hidden rounded-lg border border-default bg-raised/30"><img src={draft.image} alt={strings.billingProductPdfPreview} className={imageClasses(draft)} /></div>
              <div className="min-w-0"><div className="h-3 w-3/4 rounded-full bg-default" /><div className="mt-3 h-2 w-full rounded-full bg-raised" /><div className="mt-2 h-2 w-2/3 rounded-full bg-raised" /></div>
              <div className="h-3 w-20 rounded-full bg-accent-soft" />
            </div>
          </div>
        </section>
        <aside className="flex flex-col gap-5">
          <EditorChoice label={strings.billingCropStyle} value={draft.imageFit} choices={[["cover", strings.billingFillFrame], ["contain", strings.billingShowFullImage]]} onChange={(imageFit) => onChange({ ...draft, imageFit })} />
          <fieldset>
            <legend className="mb-2 text-xs font-semibold uppercase tracking-wide text-tertiary">{strings.billingZoom}</legend>
            <div className="grid grid-cols-3 gap-2">
              {[75, 100, 125, 150, 200].map((zoom) => <button key={zoom} type="button" className={cx("min-h-10 rounded-lg border px-3 py-2 text-sm font-medium transition-colors", draft.imageZoom === zoom ? "border-accent bg-accent-soft text-accent" : "border-default bg-surface text-primary hover:border-accent/50 hover:bg-raised")} aria-pressed={draft.imageZoom === zoom} onClick={() => onChange({ ...draft, imageZoom: zoom })}>{zoom}%</button>)}
              <label className="relative"><span className="sr-only">{strings.billingCustomZoom}</span><input aria-label={strings.billingCustomZoom} type="number" min="50" max="200" step="10" value={draft.imageZoom} className="min-h-10 w-full rounded-lg border border-default bg-surface px-3 pr-7 text-sm font-medium text-primary focus:border-accent focus:outline-none" onChange={(event) => onChange({ ...draft, imageZoom: event.currentTarget.valueAsNumber })} onBlur={(event) => onChange({ ...draft, imageZoom: normalizeZoom(event.currentTarget.valueAsNumber) })} /><span className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-xs text-tertiary">%</span></label>
            </div>
            <p className="mt-2 text-xs leading-relaxed text-secondary">{strings.billingZoomHelp}</p>
          </fieldset>
          <EditorChoice label={strings.billingFocusArea} value={draft.imagePosition} choices={[["center", strings.billingCentre], ["top", strings.billingTop], ["bottom", strings.billingBottom], ["left", strings.billingLeft], ["right", strings.billingRight]]} onChange={(imagePosition) => onChange({ ...draft, imagePosition })} />
        </aside>
      </div>
    </Modal>
  );
}
