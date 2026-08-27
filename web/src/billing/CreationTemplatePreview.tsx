import type { CreationTemplate } from "./DocumentEditor";

export function CreationTemplatePreview({
  kind,
}: {
  kind: CreationTemplate["preview"];
}) {
  const line = "h-1 rounded-full bg-[#CBD5E1]";
  const strongLine = "h-1.5 rounded-full bg-[#102A43]";
  const accentLine = "h-1 rounded-full bg-[#E76F51]";

  if (kind === "blank") {
    return (
      <span className="flex h-full items-center justify-center rounded-xl bg-white">
        <span className="flex size-10 items-center justify-center rounded-xl bg-[#FCE9E3] text-xl font-medium text-[#E76F51]">
          +
        </span>
      </span>
    );
  }

  if (kind === "project") {
    return (
      <span className="flex h-full flex-col overflow-hidden rounded-xl bg-white">
        <span className="grid grid-cols-[0.65fr_1fr] bg-[#E76F51] px-3 py-2">
          <span className="h-1.5 w-1/2 rounded-full bg-white/90" />
          <span className="ml-auto h-1 w-1/3 rounded-full bg-white/70" />
        </span>
        <span className="grid flex-1 grid-cols-[0.65fr_1fr] gap-3 p-3">
          <span className="flex flex-col gap-2 rounded-lg bg-[#FCE9E3] p-2">
            <span className={`${strongLine} w-3/4`} />
            <span className={`${line} w-full`} />
            <span className={`${line} w-4/5`} />
            <span className={`${line} w-2/3`} />
          </span>
          <span className="flex flex-col gap-2">
            <span className={`${strongLine} w-2/3`} />
            <span className="grid grid-cols-3 gap-1">
              {[0, 1, 2].map((item) => (
                <span key={item} className="h-5 rounded bg-[#F3F0EA]" />
              ))}
            </span>
            <span className="mt-auto grid gap-1">
              {[0, 1].map((row) => (
                <span
                  key={row}
                  className="grid grid-cols-[1fr_0.3fr] gap-2 border-t border-[#E7E1D8] pt-1"
                >
                  <span className={`${line} w-full`} />
                  <span className={`${strongLine} w-full`} />
                </span>
              ))}
            </span>
          </span>
        </span>
      </span>
    );
  }

  if (kind === "retainer") {
    return (
      <span className="flex h-full flex-col rounded-xl bg-white p-3">
        <span className="flex items-start justify-between gap-3">
          <span className="flex flex-1 flex-col gap-2">
            <span className={`${strongLine} w-2/3`} />
            <span className={`${line} w-1/2`} />
          </span>
          <span className="rounded-lg bg-[#FCE9E3] px-2 py-1 text-[9px] font-semibold text-[#E76F51]">
            12×
          </span>
        </span>
        <span className="mt-3 flex flex-1 flex-col justify-center gap-2 rounded-lg bg-[#F3F0EA] px-3">
          <span className="flex items-center justify-between gap-3">
            <span className={`${line} w-2/5`} />
            <span className={`${strongLine} w-1/4`} />
          </span>
          <span className="flex items-center justify-between gap-3">
            <span className={`${line} w-1/2`} />
            <span className={`${accentLine} w-1/5`} />
          </span>
        </span>
      </span>
    );
  }

  return (
    <span className="flex h-full flex-col overflow-hidden rounded-xl bg-white p-3">
      <span className="mb-2 grid grid-cols-[1fr_0.8fr] gap-3 border-b border-[#E7E1D8] pb-2">
        <span className="flex items-center gap-2">
          <span className="size-5 rounded-md bg-[#FCE9E3]" />
          <span className={`${strongLine} w-1/2`} />
        </span>
        <span className="flex flex-col items-end gap-1">
          <span className={`${accentLine} w-1/3`} />
          <span className={`${line} w-1/2`} />
        </span>
      </span>
      <span className="mb-1.5 grid gap-1">
        <span className={`${strongLine} w-2/5`} />
        <span className={`${line} w-4/5`} />
      </span>
      <span className="mt-auto rounded-lg bg-[#F3F0EA] px-2 py-1.5">
        {["w-full", "w-3/4"].map((width) => (
          <span
            key={width}
            className="grid grid-cols-[1fr_0.3fr] items-center gap-2 border-t border-[#E7E1D8] py-1"
          >
            <span className={`${line} ${width}`} />
            <span className={`${strongLine} w-full`} />
          </span>
        ))}
      </span>
    </span>
  );
}
