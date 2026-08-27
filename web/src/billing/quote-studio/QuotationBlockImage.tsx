import { cx } from "../../ds";
import { strings } from "../../i18n";
import { IMAGE_BLOCK_ZOOM, IMAGE_FRAME } from "./imageBlockLayout";
import type { ImageBlock } from "./QuoteStudioBlock";

export function QuotationBlockImage({
  block,
  onDoubleClick,
}: {
  block: ImageBlock;
  onDoubleClick?: () => void;
}) {
  const aspect = block.aspect ?? "landscape";
  const fit = block.fit ?? "cover";
  const zoom = fit === "cover" ? Math.max(100, block.zoom ?? 100) : (block.zoom ?? 100);

  return (
    <div className={cx("relative overflow-hidden rounded-xl bg-surface", IMAGE_FRAME[aspect])}>
      <img
        src={block.src}
        alt={block.caption || strings.quoteStudioQuotationImageAlt}
        className={cx(
          "transition-transform duration-200",
          aspect === "natural" ? "mx-auto max-h-[520px] w-full" : "absolute inset-0 size-full",
          fit === "contain" ? "object-contain" : "object-cover",
          IMAGE_BLOCK_ZOOM[zoom as keyof typeof IMAGE_BLOCK_ZOOM],
        )}
        onDoubleClick={onDoubleClick}
      />
    </div>
  );
}
