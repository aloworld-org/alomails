export type HeaderStyle =
  | "signature"
  | "editorial"
  | "band"
  | "minimal"
  | "stacked";

export function HeaderStylePreview({ style }: { style: HeaderStyle }) {
  if (style === "editorial") {
    return (
      <span className="flex h-20 items-end justify-between rounded-xl bg-raised p-3" aria-hidden="true">
        <span className="space-y-2">
          <span className="block h-2 w-20 rounded-full bg-primary/25" />
          <span className="block h-1.5 w-12 rounded-full bg-primary/10" />
        </span>
        <span className="mb-auto size-7 rounded-lg bg-accent-soft" />
      </span>
    );
  }

  if (style === "band") {
    return (
      <span className="flex h-20 overflow-hidden rounded-xl bg-raised" aria-hidden="true">
        <span className="flex w-2/5 flex-col justify-center gap-2 bg-accent px-3">
          <span className="block size-6 rounded-md bg-white/80" />
          <span className="block h-1.5 w-12 rounded-full bg-white/70" />
        </span>
        <span className="flex flex-1 flex-col justify-center gap-2 px-3">
          <span className="block h-2 w-16 rounded-full bg-primary/25" />
          <span className="block h-1.5 w-10 rounded-full bg-primary/10" />
        </span>
      </span>
    );
  }

  if (style === "minimal") {
    return (
      <span className="flex h-20 items-center justify-between border-y border-default px-2" aria-hidden="true">
        <span className="flex items-center gap-2">
          <span className="size-6 rounded-full border border-accent/40" />
          <span className="block h-1.5 w-12 rounded-full bg-primary/20" />
        </span>
        <span className="block h-1.5 w-12 rounded-full bg-accent" />
      </span>
    );
  }

  if (style === "stacked") {
    return (
      <span className="grid h-20 grid-cols-[0.8fr_1.2fr] overflow-hidden rounded-xl bg-raised" aria-hidden="true">
        <span className="flex flex-col items-center justify-center gap-1.5">
          <span className="size-7 rounded-lg bg-accent-soft" />
          <span className="block h-1.5 w-12 rounded-full bg-primary/20" />
        </span>
        <span className="flex flex-col justify-center gap-2 border-l border-default px-3">
          <span className="block h-2 w-16 rounded-full bg-primary/25" />
          <span className="block h-1.5 w-10 rounded-full bg-accent/60" />
        </span>
      </span>
    );
  }

  return (
    <span className="grid h-20 grid-cols-[1.1fr_0.9fr] overflow-hidden rounded-xl bg-raised" aria-hidden="true">
      <span className="flex items-center gap-2.5 px-3">
        <span className="size-7 rounded-lg bg-accent-soft" />
        <span className="space-y-1.5">
          <span className="block h-2 w-14 rounded-full bg-primary/25" />
          <span className="block h-1.5 w-10 rounded-full bg-primary/10" />
        </span>
      </span>
      <span className="flex flex-col justify-center gap-2 border-l border-default px-3">
        <span className="block h-1.5 w-12 rounded-full bg-accent/60" />
        <span className="block h-1.5 w-8 rounded-full bg-primary/10" />
      </span>
    </span>
  );
}
