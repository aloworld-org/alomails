// Where the AI copy tools learn which page, and which section of it, the
// field they sit under belongs to.
//
// It is its own module because two files need it — the prop forms and the
// image fields — and neither should have to import the other to get it.
// A null context is the ordinary case, not an error: a section being added
// has no stored target yet, and a field with no target offers no AI tool.
import { createContext, useContext } from "react";

import type { SectionsEnvelope } from "./sections";
import type { SiteEditTarget } from "./types";

export interface CopyContextValue {
  siteId: string;
  pageId: string;
  target: SiteEditTarget;
  onApplied: (sections: SectionsEnvelope) => void;
}

export const CopyContext = createContext<CopyContextValue | null>(null);

/** The page and section the surrounding form is editing, or `null` when the
 *  section has never been saved. */
export function useCopyContext(): CopyContextValue | null {
  return useContext(CopyContext);
}
