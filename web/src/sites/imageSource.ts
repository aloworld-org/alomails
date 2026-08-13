// One of the tenant's image blobs, as an object URL a form can show.
//
// It lived inside `ImageFields.tsx` while the section forms were the only
// screen that showed a stored picture back; the catalog item dialog is the
// second, and a copy of it there would be a second thing to fix the day the
// image route changes.
import { useEffect, useState } from "react";

import { useSitesApi } from "./api";

/**
 * Loads one of the tenant's image blobs as an object URL for as long as the
 * caller is mounted, and revokes it after. A picture that will not load is not
 * an error anybody can act on — the caller shows its own placeholder and the
 * rest of the form keeps working — so the failure is `null`, not a throw.
 *
 * A blank `blobId` (or a blank `siteId`) means "no picture", and loads nothing.
 */
export function useImageSource(siteId: string, blobId: string): string | null {
  const api = useSitesApi();
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    const id = blobId.trim();
    if (siteId === "" || id === "" || typeof URL.createObjectURL !== "function") {
      setUrl(null);
      return;
    }
    let live = true;
    let created: string | null = null;
    api.siteImage(siteId, id).then(
      (blob) => {
        if (!live) return;
        created = URL.createObjectURL(blob);
        setUrl(created);
      },
      () => {
        if (live) setUrl(null);
      },
    );
    return () => {
      live = false;
      setUrl(null);
      if (created !== null) URL.revokeObjectURL(created);
    };
  }, [api, siteId, blobId]);

  return url;
}
