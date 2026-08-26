import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import { AlignLeft, Check, Heading2, ImagePlus, Palette, Plus, Trash2, Upload, X } from "lucide-react";

import { Button, Input, Modal, cx } from "../ds";

type Theme = "modern" | "editorial" | "minimal";
type Block =
  | { id: string; kind: "text"; heading: string; body: string }
  | { id: string; kind: "image"; src: string; caption: string };
interface Colors { accent: string; background: string; text: string; tableHeader: string; tableRows: string }
interface Design { logo: string; theme: Theme; colors: Colors; blocks: Block[] }
const DEFAULT_COLORS: Colors = { accent: "#e76f51", background: "#fffefc", text: "#102a43", tableHeader: "#f3f0ea", tableRows: "#fffefc" };
const EMPTY: Design = { logo: "", theme: "modern", colors: DEFAULT_COLORS, blocks: [] };
const DESIGN_STORE = "quote-designs";
const DESIGN_DATABASE = "alo-quote-assets";
const themeChoices: Array<{ id: Theme; name: string; help: string }> = [
  { id: "modern", name: "Modern", help: "Clean and confident" },
  { id: "editorial", name: "Editorial", help: "Story-led headings" },
  { id: "minimal", name: "Minimal", help: "Quiet and precise" },
];

function legacyDesign(key: string): Design | null {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return null;
    const saved = JSON.parse(raw) as Partial<Design>;
    return { ...EMPTY, ...saved, colors: { ...DEFAULT_COLORS, ...saved.colors } };
  } catch { return null; }
}

function designDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DESIGN_DATABASE, 1);
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(DESIGN_STORE)) request.result.createObjectStore(DESIGN_STORE);
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("The quotation design database could not be opened."));
  });
}

async function loadDesign(key: string): Promise<Design> {
  try {
    const database = await designDatabase();
    const saved = await new Promise<Partial<Design> | undefined>((resolve, reject) => {
      const request = database.transaction(DESIGN_STORE, "readonly").objectStore(DESIGN_STORE).get(key);
      request.onsuccess = () => resolve(request.result as Partial<Design> | undefined);
      request.onerror = () => reject(request.error);
    });
    database.close();
    if (saved !== undefined) return { ...EMPTY, ...saved, colors: { ...DEFAULT_COLORS, ...saved.colors } };
  } catch { /* Fall through to the small legacy record when IndexedDB is unavailable. */ }
  return legacyDesign(key) ?? EMPTY;
}

async function saveDesign(key: string, design: Design): Promise<void> {
  const database = await designDatabase();
  await new Promise<void>((resolve, reject) => {
    const transaction = database.transaction(DESIGN_STORE, "readwrite");
    transaction.objectStore(DESIGN_STORE).put(design, key);
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error ?? new Error("The quotation design could not be saved."));
    transaction.onabort = () => reject(transaction.error ?? new Error("The quotation design save was cancelled."));
  });
  database.close();
  localStorage.removeItem(key);
}
function imageData(file: File, done: (value: string) => void) {
  const reader = new FileReader();
  reader.onload = () => typeof reader.result === "string" && done(reader.result);
  reader.readAsDataURL(file);
}

export interface QuoteContentStudioHandle { customize: () => void }

export const QuoteContentStudio = forwardRef<QuoteContentStudioHandle, { quoteId: string; readOnly: boolean; preview?: boolean }>(function QuoteContentStudio({ quoteId, readOnly, preview = false }, ref) {
  const storageKey = `alo:quote-design:${quoteId}`;
  const [design, setDesign] = useState<Design>(EMPTY);
  const [ready, setReady] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [customize, setCustomize] = useState(false);
  const [insertAt, setInsertAt] = useState<number | null>(null);
  const root = useRef<HTMLElement>(null);
  const imageInput = useRef<HTMLInputElement>(null);
  const pendingImageIndex = useRef<number | null>(null);
  useImperativeHandle(ref, () => ({ customize: () => setCustomize(true) }), []);

  useEffect(() => {
    let current = true;
    setReady(false);
    void loadDesign(storageKey).then((saved) => {
      if (!current) return;
      setDesign(saved);
      setReady(true);
    });
    return () => { current = false; };
  }, [storageKey]);
  useEffect(() => {
    if (!ready) return;
    let current = true;
    const timeout = window.setTimeout(() => {
      void saveDesign(storageKey, design).then(() => {
        if (current) setSaveError("");
      }).catch(() => {
        if (current) setSaveError("This design could not be saved. Try a smaller image or upload it again.");
      });
    }, 200);
    return () => { current = false; window.clearTimeout(timeout); };
  }, [design, ready, storageKey]);
  useEffect(() => {
    const document = root.current?.closest("article");
    if (!(document instanceof HTMLElement)) return;
    const values = { "--quote-accent": design.colors.accent, "--quote-background": design.colors.background, "--quote-text": design.colors.text, "--quote-table-header": design.colors.tableHeader, "--quote-table-row": design.colors.tableRows };
    Object.entries(values).forEach(([name, value]) => document.style.setProperty(name, value));
  }, [design.colors]);

  const insertBlock = (index: number, block: Block) => setDesign((current) => ({ ...current, blocks: [...current.blocks.slice(0, index), block, ...current.blocks.slice(index)] }));
  const addText = (index: number, heading = "") => {
    insertBlock(index, { id: crypto.randomUUID(), kind: "text", heading, body: "" });
    setInsertAt(null);
  };
  const chooseImage = (index: number) => {
    pendingImageIndex.current = index;
    setInsertAt(null);
    imageInput.current?.click();
  };
  const update = (id: string, patch: Partial<Block>) => setDesign((current) => ({ ...current, blocks: current.blocks.map((block) => block.id === id ? { ...block, ...patch } as Block : block) }));

  return <>
    <section ref={root} className="overflow-hidden rounded-2xl border border-default bg-surface shadow-sm">
      {!preview && <header className="flex flex-wrap items-center justify-between gap-4 border-b border-subtle px-6 py-4 max-md:px-4">
        <div><h2 className="text-base font-semibold text-primary">Proposal content</h2><p className="mt-0.5 text-sm text-secondary">Add the story and imagery your customer needs.</p></div>
      </header>}
      <div className={cx("p-6 max-md:p-4", design.theme === "editorial" && "[&_h3]:font-editorial [&_h3]:text-2xl", design.theme === "minimal" && "[&_article]:shadow-none")}>
        {design.logo && <div className="mb-6 flex items-center justify-between border-b border-[var(--quote-table-header)] pb-5"><img src={design.logo} alt="Company logo" className="max-h-16 max-w-56 object-contain" /><span className="h-1 w-20 rounded-full bg-[var(--quote-accent)]" /></div>}
        {design.blocks.length === 0 ? <div className="flex min-h-44 flex-col items-center justify-center rounded-xl border border-dashed border-default bg-[var(--quote-background)] px-6 py-8 text-center"><h3 className="text-base font-semibold text-primary">Build the story around your price</h3><p className="mt-1 max-w-lg text-sm text-secondary">Add an introduction, scope, product image, delivery plan, or terms.</p>{!readOnly && <InsertContent open={insertAt === 0} onToggle={() => setInsertAt((current) => current === 0 ? null : 0)} onText={() => addText(0)} onHeading={() => addText(0, "Section heading")} onImage={() => chooseImage(0)} />}</div> : <div className="flex flex-col">{design.blocks.map((block, index) => <div key={block.id}><article className="group relative rounded-xl border border-[var(--quote-table-header)] bg-[var(--quote-background)] p-5 text-[var(--quote-text)] shadow-sm">
          {!readOnly && <button type="button" aria-label="Remove block" className="absolute right-3 top-3 flex size-9 items-center justify-center rounded-lg text-tertiary opacity-70 hover:bg-danger-tint hover:text-danger group-hover:opacity-100" onClick={() => setDesign((current) => ({ ...current, blocks: current.blocks.filter((item) => item.id !== block.id) }))}><Trash2 className="size-4" /></button>}
          {block.kind === "text" ? readOnly ? <><h3 className="text-lg font-semibold">{block.heading}</h3><p className="mt-2 whitespace-pre-wrap text-sm leading-relaxed opacity-80">{block.body}</p></> : <div className="pr-10"><Input value={block.heading} placeholder="Section heading" aria-label="Section heading" onChange={(event) => update(block.id, { heading: event.target.value })} /><textarea className="mt-3 min-h-24 w-full resize-y rounded-md border border-default bg-surface px-3 py-3 text-sm leading-relaxed text-primary placeholder:text-tertiary focus:border-accent focus:outline-none" value={block.body} placeholder="Write a clear paragraph for your customer…" aria-label="Section paragraph" onChange={(event) => update(block.id, { body: event.target.value })} /></div> : <><img src={block.src} alt={block.caption || "Quote image"} className="max-h-[420px] w-full rounded-lg object-cover" />{readOnly ? <p className="mt-3 text-sm opacity-80">{block.caption}</p> : <Input className="mt-3" value={block.caption} placeholder="Add an image caption" aria-label="Image caption" onChange={(event) => update(block.id, { caption: event.target.value })} />}</>}
        </article>{!readOnly && <InsertContent compact open={insertAt === index + 1} onToggle={() => setInsertAt((current) => current === index + 1 ? null : index + 1)} onText={() => addText(index + 1)} onHeading={() => addText(index + 1, "Section heading")} onImage={() => chooseImage(index + 1)} />}</div>)}</div>}
      </div>
      <input ref={imageInput} type="file" accept="image/png,image/jpeg,image/webp" className="sr-only" onChange={(event) => { const file = event.target.files?.[0]; const index = pendingImageIndex.current ?? design.blocks.length; if (file) imageData(file, (src) => insertBlock(index, { id: crypto.randomUUID(), kind: "image", src, caption: "" })); pendingImageIndex.current = null; event.currentTarget.value = ""; }} />
    </section>
    {customize && <CustomizeQuote design={design} saveError={saveError} onChange={setDesign} onClose={() => setCustomize(false)} />}
  </>;
});

function InsertContent({ open, onToggle, onText, onHeading, onImage, compact = false }: { open: boolean; onToggle: () => void; onText: () => void; onHeading: () => void; onImage: () => void; compact?: boolean }) {
  const choices = [
    { label: "Text", help: "Add a paragraph or introduction", Icon: AlignLeft, action: onText },
    { label: "Heading", help: "Start a clearly named section", Icon: Heading2, action: onHeading },
    { label: "Image", help: "Upload a product or project image", Icon: ImagePlus, action: onImage },
  ];
  return <div className={cx("relative flex w-full flex-col items-center", compact ? "py-2" : "mt-5")}>
    {compact && <span className="absolute left-0 right-0 top-1/2 border-t border-subtle" aria-hidden="true" />}
    <button type="button" className={cx("relative z-10 inline-flex items-center justify-center gap-2 rounded-full border border-accent/20 bg-surface text-sm font-semibold text-accent shadow-sm transition-colors hover:border-accent hover:bg-accent-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25", compact ? "size-9" : "min-h-11 px-5")} aria-label={compact ? "Add content here" : undefined} aria-expanded={open} onClick={onToggle}><Plus className="size-4" aria-hidden="true" />{!compact && <span>Add content</span>}</button>
    {open && <div className="relative z-20 mt-2 grid w-full max-w-2xl grid-cols-3 gap-2 rounded-2xl border border-default bg-surface p-3 text-left shadow-lg max-sm:grid-cols-1" role="menu" aria-label="Add proposal content">
      {choices.map(({ label, help, Icon, action }) => <button key={label} type="button" role="menuitem" className="flex min-h-20 items-center gap-3 rounded-xl px-3 py-2 text-left transition-colors hover:bg-accent-soft hover:text-accent focus-visible:bg-accent-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/25" onClick={action}><span className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-raised text-secondary"><Icon className="size-4" aria-hidden="true" /></span><span><strong className="block text-sm font-semibold text-primary">{label}</strong><small className="mt-0.5 block text-xs leading-relaxed text-secondary">{help}</small></span></button>)}
    </div>}
  </div>;
}

function CustomizeQuote({ design, saveError, onChange, onClose }: { design: Design; saveError: string; onChange: React.Dispatch<React.SetStateAction<Design>>; onClose: () => void }) {
  const logoInput = useRef<HTMLInputElement>(null);
  const setColor = (name: keyof Colors, value: string) => onChange((current) => ({ ...current, colors: { ...current.colors, [name]: value } }));
  return <Modal title="Customize quotation" icon={<Palette className="size-5" />} onClose={onClose} wide actions={<button type="button" className="flex size-9 items-center justify-center rounded-lg text-tertiary hover:bg-raised hover:text-primary" aria-label="Close" onClick={onClose}><X className="size-4" /></button>} footer={<><p className={cx("mr-auto text-xs", saveError ? "text-danger" : "text-secondary")}>{saveError || "Changes are saved automatically."}</p><Button onClick={onClose}>Done</Button></>}>
    <div className="grid gap-6 md:grid-cols-[220px_minmax(0,1fr)]">
      <section><h3 className="text-sm font-semibold text-primary">Logo</h3><p className="mt-1 text-xs text-secondary">PNG, JPG, WebP, or SVG.</p><button type="button" className="mt-3 flex min-h-28 w-full items-center justify-center overflow-hidden rounded-xl border border-dashed border-default bg-raised/30 p-3 text-sm font-medium text-secondary hover:border-accent hover:bg-accent-soft hover:text-accent" onClick={() => logoInput.current?.click()}>{design.logo ? <img src={design.logo} alt="Quote logo" className="max-h-20 max-w-full object-contain" /> : <span className="flex items-center gap-2"><Upload className="size-4" /> Upload logo</span>}</button>{design.logo && <button type="button" className="mt-2 text-xs font-semibold text-secondary hover:text-danger" onClick={() => onChange((current) => ({ ...current, logo: "" }))}>Remove logo</button>}<input ref={logoInput} type="file" accept="image/png,image/jpeg,image/webp,image/svg+xml" className="sr-only" onChange={(event) => { const file = event.target.files?.[0]; if (file) imageData(file, (logo) => onChange((current) => ({ ...current, logo }))); }} /></section>
      <div className="min-w-0"><div className="flex items-center justify-between gap-3"><div><h3 className="text-sm font-semibold text-primary">Document colours</h3><p className="mt-1 text-xs text-secondary">Applied only to this customer-facing quotation.</p></div><button type="button" className="text-xs font-semibold text-accent hover:text-accent-hover" onClick={() => onChange((current) => ({ ...current, colors: DEFAULT_COLORS }))}>Reset</button></div><div className="mt-4 grid grid-cols-2 gap-3 sm:grid-cols-3"><ColorField label="Accent" value={design.colors.accent} onChange={(value) => setColor("accent", value)} /><ColorField label="Page" value={design.colors.background} onChange={(value) => setColor("background", value)} /><ColorField label="Text" value={design.colors.text} onChange={(value) => setColor("text", value)} /><ColorField label="Table heading" value={design.colors.tableHeader} onChange={(value) => setColor("tableHeader", value)} /><ColorField label="Table rows" value={design.colors.tableRows} onChange={(value) => setColor("tableRows", value)} /></div>
        <h3 className="mt-6 text-sm font-semibold text-primary">Typography</h3><div className="mt-3 grid gap-3 sm:grid-cols-3">{themeChoices.map((theme) => <button key={theme.id} type="button" className={cx("flex min-h-20 items-center gap-3 rounded-xl border bg-surface px-3 py-3 text-left hover:border-accent hover:bg-accent-soft", design.theme === theme.id ? "border-accent shadow-[inset_0_0_0_1px_var(--accent)]" : "border-default")} onClick={() => onChange((current) => ({ ...current, theme: theme.id }))}><span className="min-w-0 flex-1"><strong className="block text-sm font-semibold text-primary">{theme.name}</strong><small className="mt-1 block text-xs text-secondary">{theme.help}</small></span>{design.theme === theme.id && <Check className="size-4 shrink-0 text-accent" />}</button>)}</div>
      </div>
    </div>
  </Modal>;
}

function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  const valid = /^#[0-9a-f]{6}$/i.test(value);
  return <div className="rounded-xl border border-default bg-surface p-3 transition-colors hover:border-accent">
    <label className="block text-xs font-semibold text-primary" htmlFor={`quote-colour-${label.replace(/\s+/g, "-").toLowerCase()}`}>{label}</label>
    <div className="mt-2 flex items-center gap-2">
      <input type="color" value={valid ? value : DEFAULT_COLORS.accent} aria-label={`Choose ${label.toLowerCase()} colour`} title={`Choose ${label.toLowerCase()} colour`} className="size-11 shrink-0 cursor-pointer rounded-lg border border-default bg-surface p-1 shadow-sm" onChange={(event) => onChange(event.target.value)} />
      <input id={`quote-colour-${label.replace(/\s+/g, "-").toLowerCase()}`} value={value.toUpperCase()} aria-label={`${label} hex colour`} className="h-11 min-w-0 w-full rounded-lg border border-default bg-surface px-3 font-mono text-xs uppercase text-primary focus:border-accent focus:outline-none" maxLength={7} spellCheck={false} onChange={(event) => { const next = event.target.value.startsWith("#") ? event.target.value : `#${event.target.value}`; onChange(next.slice(0, 7)); }} />
    </div>
  </div>;
}
