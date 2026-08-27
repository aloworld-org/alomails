import { cx } from "../../ds";
import type {
  DividerBlock,
  DividerThickness,
  DividerWidth,
} from "./QuoteStudioBlock";

const thicknessClasses: Record<DividerThickness, string> = {
  fine: "border-t",
  medium: "border-t-2",
  bold: "border-t-4",
};

const widthClasses: Record<DividerWidth, string> = {
  25: "w-1/4",
  50: "w-1/2",
  75: "w-3/4",
  100: "w-full",
};

export function DividerLine({ block }: { block: DividerBlock }) {
  const thickness = block.thickness ?? "fine";
  const width = block.width ?? 100;
  return (
    <div
      className={cx(
        "mx-auto border-0",
        thicknessClasses[thickness],
        widthClasses[width],
      )}
      style={{
        borderTopColor: block.color ?? "var(--quote-accent)",
        borderTopStyle: block.style ?? "solid",
      }}
      aria-hidden="true"
    />
  );
}
