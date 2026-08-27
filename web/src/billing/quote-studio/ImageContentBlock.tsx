import { cx } from "../../ds";
import { IMAGE_COLUMN_GRID } from "./imageBlockLayout";
import { QuotationBlockImage } from "./QuotationBlockImage";
import type { ImageBlock } from "./QuoteStudioBlock";
import { RichTextContent } from "./RichTextContent";

export function ImageContentBlock({ block, readOnly, onEdit }: { block: ImageBlock; readOnly: boolean; onEdit: () => void }) {
  const placement = block.placement ?? "full";
  const image = (
    <figure>
      <QuotationBlockImage block={block} {...(readOnly ? {} : { onDoubleClick: onEdit })} />
      {block.caption && (
        <figcaption className="mt-2 px-1 text-xs leading-relaxed opacity-65">
          <RichTextContent value={block.caption} />
        </figcaption>
      )}
    </figure>
  );
  const copy = block.body && (
    <div className="flex flex-col justify-center px-1 py-2">
      <RichTextContent value={block.body} />
    </div>
  );

  if (placement === "full") return <div>{image}{copy && <div className="mt-4">{copy}</div>}</div>;
  return (
    <div className={cx("grid items-center gap-6", IMAGE_COLUMN_GRID[block.columnRatio ?? "50-50"][placement])}>
      {placement === "left" ? image : copy}
      {placement === "left" ? copy : image}
    </div>
  );
}
