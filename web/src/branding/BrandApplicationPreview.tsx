import { useState, type CSSProperties, type ReactNode } from "react";
import { FileText, Globe2, Megaphone } from "lucide-react";

import { strings } from "../i18n";
import { readableInk } from "./colorTools";
import type { BrandKit } from "./model";

type Preview = "website" | "document" | "campaign";

export function BrandApplicationPreview({ kit }: { kit: BrandKit }) {
  const [preview, setPreview] = useState<Preview>("website");
  const primary = kit.primary.value;
  const secondary = kit.secondary?.value ?? primary;
  const variables = {
    "--brand-primary": primary,
    "--brand-primary-ink": readableInk(primary),
    "--brand-secondary": secondary,
    "--brand-secondary-ink": readableInk(secondary),
  } as CSSProperties;

  return (
    <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm" style={variables}>
      <header className="flex flex-wrap items-center justify-between gap-5 border-b border-subtle px-5 py-4 lg:px-6">
        <div>
          <h2 className="m-0 text-lg font-semibold text-primary">{strings.brandingVisualStudio}</h2>
          <p className="mb-0 mt-1 text-sm text-secondary">{strings.brandingSeeItInUse}</p>
        </div>
        <div className="inline-flex gap-1 rounded-xl border border-subtle bg-raised p-1" role="tablist" aria-label={strings.brandingPreviewContexts}>
          {([
            ["website", strings.brandingPreviewWebsite, Globe2],
            ["document", strings.brandingPreviewDocument, FileText],
            ["campaign", strings.brandingPreviewCampaign, Megaphone],
          ] as const).map(([id, label, Icon]) => (
            <button key={id} type="button" role="tab" aria-selected={preview === id}
              className={`inline-flex min-h-9 items-center gap-2 rounded-lg px-3 text-xs font-medium transition-[background-color,color,box-shadow] ${preview === id ? "bg-surface text-primary shadow-sm ring-1 ring-inset ring-subtle" : "text-secondary hover:bg-surface/60 hover:text-primary"}`}
              onClick={() => setPreview(id)}>
              <Icon size={15} />{label}
            </button>
          ))}
        </div>
      </header>
      <div className="min-h-[34rem] bg-raised p-4 sm:p-6 lg:p-8">
        {preview === "website" && <WebsitePreview />}
        {preview === "document" && <DocumentPreview />}
        {preview === "campaign" && <CampaignPreview />}
      </div>
    </section>
  );
}

function BrandMark() {
  return <span className="grid size-8 place-items-center rounded-lg bg-[var(--brand-primary)] text-sm font-bold text-[var(--brand-primary-ink)]">A</span>;
}

function DemoButton({ secondary = false, children }: { secondary?: boolean; children: ReactNode }) {
  return <button className={secondary
    ? "min-h-10 rounded-lg border border-[var(--brand-secondary)] bg-transparent px-4 text-sm font-semibold text-[var(--brand-secondary)]"
    : "min-h-10 rounded-lg bg-[var(--brand-primary)] px-4 text-sm font-semibold text-[var(--brand-primary-ink)]"
  }>{children}</button>;
}

function WebsitePreview() {
  return (
    <div className="mx-auto overflow-hidden rounded-xl border border-black/10 bg-white shadow-xl shadow-black/10">
      <div className="flex h-8 items-center gap-1.5 border-b border-slate-200 bg-slate-50 px-4" aria-hidden="true">
        <i className="size-2 rounded-full bg-slate-300" /><i className="size-2 rounded-full bg-slate-300" /><i className="size-2 rounded-full bg-slate-300" />
        <span className="mx-auto h-2 w-36 rounded-full bg-slate-200" />
      </div>
      <nav className="flex min-h-16 items-center gap-4 px-6 text-slate-800">
        <BrandMark /><strong className="mr-auto text-sm">Atelier North</strong>
        <span className="hidden text-xs font-medium text-slate-500 sm:inline">Work</span><span className="hidden text-xs font-medium text-slate-500 sm:inline">About</span>
        <button className="rounded-lg bg-[var(--brand-primary)] px-3 py-2 text-xs font-semibold text-[var(--brand-primary-ink)]">Start a project</button>
      </nav>
      <div className="grid min-h-80 md:grid-cols-[1.15fr_0.85fr]">
        <div className="flex flex-col justify-center px-8 py-12 lg:px-12">
          <small className="font-semibold uppercase tracking-[0.12em] text-[var(--brand-primary)]">Independent design studio</small>
          <h3 className="mb-3 mt-3 max-w-xl text-3xl font-semibold leading-tight tracking-tight text-slate-900 lg:text-4xl">Ideas shaped into brands people remember.</h3>
          <p className="mb-6 max-w-lg text-sm leading-6 text-slate-500">A clear identity, thoughtful digital experiences, and a system your team can use with confidence.</p>
          <div className="flex flex-wrap gap-2"><DemoButton>Explore our work</DemoButton><DemoButton secondary>Our approach</DemoButton></div>
        </div>
        <div className="relative hidden overflow-hidden bg-[var(--brand-secondary)] md:block">
          <span className="absolute -right-10 -top-12 size-56 rounded-full border-[2.5rem] border-white/10" />
          <span className="absolute -bottom-16 -left-10 size-48 rounded-full bg-[var(--brand-primary)] opacity-90" />
          <span className="absolute bottom-10 right-10 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--brand-secondary-ink)] opacity-75">Identity · Digital · Strategy</span>
        </div>
      </div>
      <div className="grid grid-cols-3 border-t border-slate-200 bg-slate-50 px-5 py-3 text-center text-xs text-slate-500">
        <span><b className="block text-base text-[var(--brand-secondary)]">42</b> launches</span><span><b className="block text-base text-[var(--brand-secondary)]">11</b> countries</span><span><b className="block text-base text-[var(--brand-secondary)]">96%</b> referred</span>
      </div>
    </div>
  );
}

function DocumentPreview() {
  return (
    <div className="mx-auto max-w-3xl rounded-sm bg-white p-7 text-slate-800 shadow-lg sm:p-9">
      <header className="flex items-start justify-between border-b-2 border-[var(--brand-primary)] pb-5"><div className="flex items-center gap-2"><BrandMark /><strong>Atelier North</strong></div><span className="text-sm font-semibold tracking-[0.12em] text-[var(--brand-primary)]">QUOTATION</span></header>
      <div className="grid grid-cols-2 gap-5 py-6 text-sm"><div><small className="block text-slate-400">Prepared for</small><b>Northstar Studio</b></div><div className="text-right"><small className="block text-slate-400">Quote</small><b>QUO-2026-0042</b></div></div>
      <div className="divide-y divide-slate-200 border-y border-slate-200 text-sm"><span className="flex justify-between py-3">Brand strategy <b>€2,400</b></span><span className="flex justify-between py-3">Visual identity <b>€4,800</b></span><span className="flex justify-between py-3">Launch toolkit <b>€1,650</b></span></div>
      <div className="ml-auto mt-5 flex max-w-xs items-center justify-between rounded-xl bg-[var(--brand-secondary)] px-5 py-4 text-[var(--brand-secondary-ink)]"><span>Total</span><strong className="text-xl">€8,850</strong></div>
      <footer className="mt-8 border-t border-slate-200 pt-4 text-xs text-slate-400">Thank you for the opportunity to build something memorable together.</footer>
    </div>
  );
}

function CampaignPreview() {
  return (
    <div className="mx-auto max-w-xl overflow-hidden rounded-xl bg-white text-slate-800 shadow-lg">
      <div className="flex items-center gap-2 px-6 py-4"><BrandMark /><strong>Atelier North</strong></div>
      <div className="grid min-h-36 place-items-center bg-[linear-gradient(135deg,var(--brand-secondary),var(--brand-primary))]"><span className="rounded-full border border-white/40 bg-white/10 px-4 py-2 text-xs font-semibold tracking-[0.12em] text-white">NEW COLLECTION</span></div>
      <div className="px-8 py-7 text-center"><small className="font-semibold uppercase tracking-[0.08em] text-[var(--brand-primary)]">A considered new chapter</small><h3 className="mb-3 mt-2 text-2xl font-semibold text-slate-900">Designed for the way you work now.</h3><p className="mx-auto mb-5 max-w-md text-sm leading-6 text-slate-500">Discover a collection built around clarity, craft, and lasting usefulness.</p><DemoButton>See the collection</DemoButton></div>
      <footer className="bg-slate-50 px-6 py-3 text-center text-xs text-slate-400">Atelier North · Brussels</footer>
    </div>
  );
}
