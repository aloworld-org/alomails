import { ImagePlus, Upload } from "lucide-react";
import { Button, Modal, cx } from "../../ds";
import { strings } from "../../i18n";
import { IMAGE_COLUMN_GRID } from "./imageBlockLayout";
import { ImageColumnRatioPicker } from "./ImageColumnRatioPicker";
import { ImageOptionGroup } from "./ImageOptionGroup";
import { ImageZoomControl } from "./ImageZoomControl";
import { QuotationBlockImage } from "./QuotationBlockImage";
import type { ImageBlock } from "./QuoteStudioBlock";
import { RichTextEditor } from "./RichTextEditor";

export function ImageBlockEditor({ block, onChange, onReplace, onClose }: { block: ImageBlock; onChange: (patch: Partial<ImageBlock>) => void; onReplace: () => void; onClose: () => void }) {
  return (
    <Modal title={strings.quoteStudioEditContentBlock} icon={<ImagePlus className="size-5" />} onClose={onClose} wide="extra" footer={<><p className="mr-auto text-xs text-secondary">{strings.quoteStudioChangesImmediate}</p><Button onClick={onClose}>{strings.quoteStudioDone}</Button></>}>
      <div className="flex flex-wrap items-end justify-between gap-3">
        <h3 className="text-base font-semibold text-primary">{strings.quoteStudioComposeImageText}</h3>
        <p className="w-full text-sm text-secondary">{strings.quoteStudioComposeImageTextHelp}</p>
      </div>
      <section className="border-y border-subtle py-5">
        <h4 className="text-sm font-semibold text-primary">{strings.quoteStudioLayoutTools}</h4>
        <p className="mt-1 text-xs text-secondary">{strings.quoteStudioLayoutToolsHelp}</p>
        <div className="mt-5 flex flex-col gap-5">
          <div className="grid items-start gap-6 md:grid-cols-[minmax(0,3fr)_minmax(0,5fr)]">
            <ImageOptionGroup label={strings.quoteStudioComposition} visual="composition" value={block.placement ?? "full"} options={[["full", strings.quoteStudioBelowImage], ["left", strings.quoteStudioImageLeft], ["right", strings.quoteStudioImageRight]]} onChange={(placement) => onChange({ placement })} />
            <ImageColumnRatioPicker value={block.columnRatio ?? "50-50"} placement={block.placement ?? "full"} onChange={(columnRatio) => onChange({ columnRatio })} />
          </div>
          <div className="grid items-start gap-6 md:grid-cols-[minmax(0,3fr)_minmax(0,2fr)_minmax(0,3fr)]">
            <ImageOptionGroup label={strings.quoteStudioImageFrame} visual="frame" value={block.aspect ?? "landscape"} options={[["natural", strings.quoteStudioNatural], ["landscape", strings.quoteStudioWide], ["square", strings.quoteStudioSquare]]} onChange={(aspect) => onChange({ aspect })} />
            <ImageOptionGroup label={strings.quoteStudioFit} visual="fit" value={block.fit ?? "cover"} options={[["cover", strings.quoteStudioFillFrame], ["contain", strings.quoteStudioWholeImage]]} onChange={(fit) => onChange({ fit, zoom: fit === "cover" && (block.zoom ?? 100) < 100 ? 100 : (block.zoom ?? 100) })} />
            <ImageZoomControl value={block.fit === "cover" ? Math.max(100, block.zoom ?? 100) as Exclude<ImageBlock["zoom"], undefined> : (block.zoom ?? 100)} minimum={block.fit === "cover" ? 100 : 50} onChange={(zoom) => onChange({ zoom })} />
          </div>
        </div>
      </section>
      <div className={cx("grid items-start gap-6", (block.placement ?? "full") === "full" ? "md:grid-cols-2" : IMAGE_COLUMN_GRID[block.columnRatio ?? "50-50"][block.placement === "right" ? "right" : "left"])}>
        <section className="min-w-0">
          <div className="mb-2 flex min-h-10 items-center justify-between gap-3">
            <h4 className="text-sm font-semibold text-primary">{strings.quoteStudioImage}</h4>
            <button type="button" className="inline-flex min-h-9 items-center gap-2 rounded-lg border border-default bg-surface px-3 text-xs font-semibold text-secondary transition-colors hover:border-accent hover:bg-accent-soft hover:text-accent" onClick={onReplace}>
              <Upload className="size-4" aria-hidden="true" /> {strings.quoteStudioReplace}
            </button>
          </div>
          <div className="rounded-2xl border border-default bg-surface p-3 shadow-sm"><QuotationBlockImage block={block} /></div>
        </section>
        <section className="min-w-0">
          <RichTextEditor value={block.body ?? ""} placeholder={strings.quoteStudioImageDescriptionPlaceholder} onChange={(body) => onChange({ body })} />
          <div className="mt-4"><RichTextEditor value={block.caption} label={strings.quoteStudioCaption} placeholder={strings.quoteStudioCaptionPlaceholder} onChange={(caption) => onChange({ caption })} /></div>
        </section>
      </div>
    </Modal>
  );
}
