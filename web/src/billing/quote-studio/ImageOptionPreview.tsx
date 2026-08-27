import { cx } from "../../ds";

interface ImageOptionPreviewProps {
  kind: "composition" | "frame" | "fit";
  option: string;
}

export function ImageOptionPreview({ kind, option }: ImageOptionPreviewProps) {
  if (kind === "composition") {
    if (option === "full") {
      return (
        <span className="mx-auto flex h-10 max-w-24 flex-col gap-1 rounded-md bg-raised p-1.5">
          <span className="h-4 rounded-sm bg-accent/25" />
          <span className="h-1 rounded-full bg-tertiary/30" />
          <span className="h-1 w-3/4 rounded-full bg-tertiary/20" />
        </span>
      );
    }
    const imageFirst = option === "left";
    return (
      <span className="mx-auto flex h-10 max-w-24 gap-1 rounded-md bg-raised p-1.5">
        <span className={cx("w-2/5 rounded-sm bg-accent/25", imageFirst ? "order-1" : "order-2")} />
        <span className={cx("flex w-3/5 flex-col justify-center gap-1", imageFirst ? "order-2" : "order-1")}>
          <span className="h-1 rounded-full bg-tertiary/30" />
          <span className="h-1 w-4/5 rounded-full bg-tertiary/20" />
          <span className="h-1 w-2/3 rounded-full bg-tertiary/20" />
        </span>
      </span>
    );
  }

  if (kind === "frame") {
    return (
      <span className="mx-auto flex h-10 max-w-24 items-center justify-center rounded-md bg-raised p-1.5">
        <span className={cx(
          "border border-accent/30 bg-accent/25",
          option === "natural" && "h-7 w-5 rounded-sm",
          option === "landscape" && "h-5 w-full rounded-sm",
          option === "square" && "size-7 rounded-sm",
        )} />
      </span>
    );
  }

  return (
    <span className="mx-auto flex h-10 max-w-24 items-center justify-center overflow-hidden rounded-md border border-subtle bg-surface p-1">
      <span className={cx("bg-accent/25", option === "cover" ? "size-full rounded-sm" : "h-6 w-3/5 rounded-sm")} />
    </span>
  );
}
