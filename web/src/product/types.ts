// The product-surface contract (ADR 0019). A "product" — the alo workspace, or
// the standalone alomails — is defined by ONE surface object: which modules it
// shows, which full-screen consoles it mounts, and which compose-editor inserts
// it offers. App.tsx, the rail, the account menu, and the editor read the
// surface generically, so trimming a product to a subset is a matter of
// swapping the surface (and deleting the now-unreferenced module source) — never
// editing the router or the editor by hand.
import type { ComponentType, ReactNode } from "react";
import type { LucideIcon } from "lucide-react";

/** A capability a console can require before it is offered in the account menu. */
export type Capability = "admin" | "operator";

/** A rail module: a nav-rail entry mounted at its path under the app shell. */
export interface ProductModule {
  /** Stable id (rail key). */
  id: string;
  /** Route path under the shell, e.g. `/mail`. */
  path: string;
  /** Rail + header label (resolved i18n string). */
  label: string;
  /** Rail icon. */
  Icon: LucideIcon;
  /** False → the rail shows it but the route renders a "coming soon" placeholder. */
  enabled: boolean;
  /** The surface to render; absent → the "coming soon" placeholder. */
  element?: () => ReactNode;
}

/** A full-screen console with its own shell (e.g. tenant admin, control plane). */
export interface ProductConsole {
  /** Route path, e.g. `/admin/*`. */
  path: string;
  /** The console element. */
  element: () => ReactNode;
  /** When present, an account-menu entry gated on `requires`. */
  menu?: { to: string; label: string; Icon: LucideIcon; requires: Capability };
}

/** A compose-editor insert action (e.g. equation/code — a Docs feature). */
export interface ComposeInsert {
  id: string;
  label: string;
  Icon: LucideIcon;
  /** A modal that inserts HTML at the caret and closes itself. */
  Modal: ComponentType<{ onInsert: (html: string) => void; onClose: () => void }>;
}

/** Login brand-panel copy. Each getter resolves an i18n string at call time,
 *  so the panel speaks the product's own language and still follows the active
 *  locale — `alomails` reads as a mail product, the suite as a workspace. */
export interface ProductBrand {
  headline: () => string;
  subtitle: () => string;
  euBadge: () => string;
}

/** Login-form options that differ by product — so the consumer mail product
 *  doesn't wear the workspace's business clothes (SSO, "your domain"). */
export interface ProductLogin {
  /** Offer "Sign in with SSO" — an enterprise feature; off for consumer mail. */
  sso: boolean;
  /** Email-field placeholder, resolved at render (i18n). */
  emailPlaceholder: () => string;
}

/** The complete definition of a product. */
export interface ProductSurface {
  modules: ProductModule[];
  consoles: ProductConsole[];
  composeInserts: ComposeInsert[];
  /** Where a bare `/` (and post-login) should land. */
  defaultPath: string;
  /** Brand copy for the login screen. */
  brand: ProductBrand;
  /** Login-form options (SSO, email placeholder). */
  login: ProductLogin;
}
