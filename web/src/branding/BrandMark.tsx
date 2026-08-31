import { presentedBrandName } from "./brandPresentation";
import { primaryBrandLogo, type BrandKit } from "./model";

export function BrandMark({ kit, large = false }: { kit: BrandKit; large?: boolean }) {
  const size = large ? "size-16 rounded-xl" : "size-9 rounded-lg";
  const logo = primaryBrandLogo(kit);
  if (logo !== null) {
    return <span className={`grid shrink-0 place-items-center overflow-hidden border border-black/5 bg-white ${size}`}><img className="max-h-[82%] max-w-[82%] object-contain" src={logo.dataUrl} alt={logo.name} /></span>;
  }
  return <span className={`grid shrink-0 place-items-center bg-[var(--brand-primary)] font-bold text-[var(--brand-primary-ink)] ${size}`} aria-hidden="true">{presentedBrandName(kit).slice(0, 1).toUpperCase()}</span>;
}
