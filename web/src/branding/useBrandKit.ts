import { useState } from "react";

import { brandKitIsValid, type BrandKit } from "./model";
import { readBrandKit, saveBrandKit } from "./repository";

export function useBrandKit() {
  const [saved, setSaved] = useState<BrandKit>(readBrandKit);
  const [draft, setDraft] = useState<BrandKit>(saved);
  const [savedNotice, setSavedNotice] = useState(false);
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);

  function save() {
    if (!brandKitIsValid(draft)) return;
    const next = saveBrandKit(draft);
    setSaved(next);
    setDraft(next);
    setSavedNotice(true);
    window.setTimeout(() => setSavedNotice(false), 2400);
  }

  return {
    draft,
    setDraft,
    dirty,
    valid: brandKitIsValid(draft),
    savedNotice,
    save,
  };
}
