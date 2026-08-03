// The alomails product surface: the Mail product on its own. Home + Mail, the
// tenant-admin console, no suite modules, no Docs-editor inserts. Because it
// imports nothing suite-only, alomails builds from this surface after the
// `control`/`authoring` source and the `workplace` surface are removed.
import { strings } from "../i18n";
import type { ProductSurface } from "./types";
import { adminConsole, defaultPath, sharedModules } from "./shared";

export const surface: ProductSurface = {
  modules: sharedModules,
  consoles: [adminConsole],
  composeInserts: [],
  defaultPath,
  brand: {
    headline: () => strings.brandHeadlineMail,
    subtitle: () => strings.brandSubtitleMail,
    euBadge: () => strings.brandEuBadgeMail,
  },
  // Consumer product: no enterprise SSO, and a personal address placeholder.
  login: {
    sso: false,
    emailPlaceholder: () => strings.emailPlaceholderMail,
  },
};
