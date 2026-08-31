import type { ReactNode } from "react";

export function PaletteInsightCard({ title, meta, children }: { title: string; meta: string; children: ReactNode }) {
  return (
    <section className="min-w-0 p-5">
      <div className="flex items-center justify-between gap-3"><h4 className="m-0 text-sm font-semibold text-primary">{title}</h4><span className="text-[0.68rem] font-semibold uppercase tracking-wide text-tertiary">{meta}</span></div>
      {children}
    </section>
  );
}
