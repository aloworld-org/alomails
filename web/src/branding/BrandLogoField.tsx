import { ImagePlus, UploadCloud } from "lucide-react";
import { useRef, useState, type DragEvent } from "react";

import { strings } from "../i18n";
import { BrandLogoCard } from "./BrandLogoCard";
import { ACCEPTED_LOGO_TYPES, readLogoFile, validateLogoFile } from "./logoFiles";
import { MAX_BRAND_LOGOS, renameBrandLogo, type BrandLogo } from "./model";

export function BrandLogoField({
  logos,
  primaryLogoId,
  onChange,
}: {
  logos: BrandLogo[];
  primaryLogoId: string | null;
  onChange: (logos: BrandLogo[], primaryLogoId: string | null) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [error, setError] = useState("");
  const [dragging, setDragging] = useState(false);

  const errorFor = (file: File) => {
    const issue = validateLogoFile(file);
    return issue === "too-large" ? strings.brandingLogoTooLarge : issue === "unsupported" ? strings.brandingLogoUnsupported : "";
  };

  async function addFiles(files: File[]) {
    const available = MAX_BRAND_LOGOS - logos.length;
    if (available <= 0) {
      setError(strings.brandingLogoLimit);
      return;
    }
    const selected = files.slice(0, available);
    const invalid = selected.find((file) => validateLogoFile(file) !== null);
    if (invalid !== undefined) {
      setError(errorFor(invalid));
      return;
    }
    try {
      const stamp = Date.now();
      const additions = await Promise.all(selected.map((file, index) => readLogoFile(file, `logo-${stamp}-${index + 1}`)));
      const next = [...logos, ...additions];
      setError(files.length > available ? strings.brandingLogoLimit : "");
      onChange(next, primaryLogoId ?? additions[0]?.id ?? null);
    } catch {
      setError(strings.brandingLogoUnsupported);
    }
  }

  async function replaceLogo(current: BrandLogo, file: File) {
    const issue = errorFor(file);
    if (issue !== "") {
      setError(issue);
      return;
    }
    try {
      const replacement = await readLogoFile(file, current.id);
      setError("");
      onChange(logos.map((logo) => logo.id === current.id ? replacement : logo), primaryLogoId);
    } catch {
      setError(strings.brandingLogoUnsupported);
    }
  }

  function handleDrop(event: DragEvent<HTMLButtonElement>) {
    event.preventDefault();
    setDragging(false);
    void addFiles(Array.from(event.dataTransfer.files));
  }

  return (
    <section className="rounded-2xl border border-subtle bg-surface p-5 shadow-sm sm:p-6" aria-labelledby="brand-logo-title">
      <div className="mb-5 flex flex-wrap items-end justify-between gap-3">
        <div>
          <h3 id="brand-logo-title" className="m-0 text-lg font-semibold text-primary">{strings.brandingLogoTitle}</h3>
          <p className="mb-0 mt-1 max-w-2xl text-sm leading-5 text-secondary">{strings.brandingLogoHint}</p>
        </div>
        {logos.length > 0 && <span className="rounded-full bg-raised px-3 py-1 text-xs font-semibold tabular-nums text-secondary">{strings.brandingLogoCount(logos.length, MAX_BRAND_LOGOS)}</span>}
      </div>

      <button
        type="button"
        className={`group flex min-h-32 w-full cursor-pointer flex-col items-center justify-center gap-3 rounded-2xl border border-dashed px-5 py-6 text-center outline-none transition-[border-color,background-color,box-shadow] focus-visible:ring-4 focus-visible:ring-accent/15 ${dragging ? "border-accent bg-accent-soft shadow-sm" : "border-default bg-raised hover:border-accent hover:bg-accent-soft"}`}
        onClick={() => inputRef.current?.click()}
        onDragEnter={(event) => { event.preventDefault(); setDragging(true); }}
        onDragOver={(event) => event.preventDefault()}
        onDragLeave={() => setDragging(false)}
        onDrop={handleDrop}
      >
        <span className="grid size-12 place-items-center rounded-xl border border-subtle bg-surface text-accent shadow-sm transition-transform group-hover:-translate-y-0.5">
          {dragging ? <UploadCloud size={22} aria-hidden="true" /> : <ImagePlus size={22} aria-hidden="true" />}
        </span>
        <span>
          <strong className="block text-sm font-semibold text-primary">{dragging ? strings.brandingLogoDropNow : strings.brandingLogoDropTitle}</strong>
          <span className="mt-1 block text-xs text-tertiary">{strings.brandingLogoRequirements}</span>
        </span>
      </button>
      <input
        ref={inputRef}
        className="sr-only"
        type="file"
        multiple
        accept={ACCEPTED_LOGO_TYPES.join(",")}
        onChange={(event) => {
          void addFiles(Array.from(event.target.files ?? []));
          event.target.value = "";
        }}
      />
      {error !== "" && <p className="mb-0 mt-3 text-sm text-danger" role="alert">{error}</p>}

      {logos.length > 0 && (
        <div className="mt-5 grid gap-4 sm:grid-cols-2">
          {logos.map((logo) => (
            <BrandLogoCard
              key={logo.id}
              logo={logo}
              primary={logo.id === primaryLogoId}
              onMakePrimary={() => onChange(logos, logo.id)}
              onRename={(label) => onChange(logos.map((candidate) => candidate.id === logo.id ? renameBrandLogo(candidate, label) : candidate), primaryLogoId)}
              onReplace={(file) => void replaceLogo(logo, file)}
              onRemove={() => {
                const next = logos.filter((candidate) => candidate.id !== logo.id);
                onChange(next, logo.id === primaryLogoId ? next[0]?.id ?? null : primaryLogoId);
              }}
            />
          ))}
        </div>
      )}
    </section>
  );
}
