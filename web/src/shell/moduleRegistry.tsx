// The module registry — the one place that declares the app's modules. The
// rail renders a button per entry; the router (see App.tsx) mounts each
// enabled module's element at its path. alomails is the Mail product, so the
// rail is Home + Mail; the other suite products (Docs, Calendar, Chat, Meet)
// live in the alo workspace, not here.
import { Home, Mail } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { strings } from "../i18n";

export interface ModuleDef {
  /** Stable id (also the rail key). */
  id: string;
  /** Route path under the shell, e.g. "/mail". */
  path: string;
  /** Rail + header label (already-resolved string from the i18n catalog). */
  label: string;
  /** Rail icon. */
  Icon: LucideIcon;
  /** False until the module is built — renders the "coming soon" placeholder. */
  enabled: boolean;
}

export const modules: ModuleDef[] = [
  { id: "home", path: "/home", label: strings.moduleHome, Icon: Home, enabled: true },
  { id: "mail", path: "/mail", label: strings.moduleMail, Icon: Mail, enabled: true },
];

/** The module a bare "/" should open. */
export const defaultModulePath = "/home";
