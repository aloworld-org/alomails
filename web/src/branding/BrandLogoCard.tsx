import { Check, RefreshCw, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { Badge, Button, IconButton } from "../ds";
import { useDialogs } from "../ds/DialogContext";
import { strings } from "../i18n";
import { ACCEPTED_LOGO_TYPES } from "./logoFiles";
import type { BrandLogo } from "./model";

export function BrandLogoCard({
  logo,
  primary,
  onMakePrimary,
  onRename,
  onReplace,
  onRemove,
}: {
  logo: BrandLogo;
  primary: boolean;
  onMakePrimary: () => void;
  onRename: (name: string) => void;
  onReplace: (file: File) => void;
  onRemove: () => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState(logo.label);
  const { confirm } = useDialogs();

  useEffect(() => setName(logo.label), [logo.label]);

  const saveName = () => {
    const next = name.trim();
    if (next === "") setName(logo.label);
    else if (next !== logo.label) onRename(next);
  };

  return (
    <article className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm transition-[border-color,box-shadow] hover:border-default hover:shadow-md">
      <div className="relative grid h-36 grid-cols-2">
        <span className="grid place-items-center bg-white p-4"><img className="max-h-24 max-w-full object-contain" src={logo.dataUrl} alt={logo.name} /></span>
        <span className="grid place-items-center bg-[#102A43] p-4"><img className="max-h-24 max-w-full object-contain" src={logo.dataUrl} alt="" /></span>
        {primary && <Badge tone="accent" className="absolute left-3 top-3"><Check size={12} aria-hidden="true" />{strings.brandingLogoPrimary}</Badge>}
      </div>
      <div className="p-4">
        <label className="block">
          <span className="text-xs font-medium text-tertiary">{strings.brandingLogoDisplayName}</span>
          <input
            value={name}
            maxLength={48}
            className="mt-1 min-h-10 w-full rounded-xl border border-default bg-surface px-3 text-sm font-semibold text-primary outline-none transition-[border-color,box-shadow] focus:border-accent focus:ring-4 focus:ring-accent/10"
            onChange={(event) => setName(event.target.value)}
            onBlur={saveName}
            onKeyDown={(event) => { if (event.key === "Enter") event.currentTarget.blur(); }}
          />
        </label>
        <span className="mt-1 block truncate text-xs text-tertiary">{logo.name}</span>
        <div className="mt-4 flex items-center gap-2">
          {!primary && <Button variant="secondary" size="sm" onClick={onMakePrimary}>{strings.brandingLogoMakePrimary}</Button>}
          <IconButton size="sm" label={strings.brandingLogoReplaceNamed(logo.label)} icon={<RefreshCw size={15} />} onClick={() => inputRef.current?.click()} />
          <IconButton
            size="sm"
            label={strings.brandingLogoRemoveNamed(logo.label)}
            icon={<Trash2 size={15} />}
            onClick={() => void confirm({
              title: strings.brandingLogoRemoveTitle,
              message: strings.brandingLogoRemoveConfirm(logo.label),
              confirmLabel: strings.brandingLogoRemove,
              danger: true,
            }).then((accepted) => { if (accepted) onRemove(); })}
          />
        </div>
        <input
          ref={inputRef}
          className="sr-only"
          type="file"
          accept={ACCEPTED_LOGO_TYPES.join(",")}
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file !== undefined) onReplace(file);
            event.target.value = "";
          }}
        />
      </div>
    </article>
  );
}
