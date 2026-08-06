// Surface pieces common to every product built on the mail core: the Home and
// Mail modules and the tenant-admin console. No suite-only imports live here,
// so this file (and everything it pulls in) ships in alomails unchanged. Drive
// is NOT here — it is its own product (alodrives), present in the workspace
// surface, not inside the mail app.
import { Calendar, Home, ListChecks, Mail, Shield } from "lucide-react";

import { strings } from "../i18n";
import { HomeModule } from "../home";
import { MailModule } from "../mail";
import { AgendaModule } from "../agenda";
import { TasksModule } from "../tasks";
import { AdminConsole } from "../admin";
import type { ProductConsole, ProductModule } from "./types";

export const sharedModules: ProductModule[] = [
  {
    id: "home",
    path: "/home",
    label: strings.moduleHome,
    Icon: Home,
    enabled: true,
    element: () => <HomeModule />,
  },
  {
    id: "mail",
    path: "/mail",
    label: strings.moduleMail,
    Icon: Mail,
    enabled: true,
    element: () => <MailModule />,
  },
  {
    id: "agenda",
    path: "/agenda",
    label: strings.moduleAgenda,
    Icon: Calendar,
    enabled: true,
    element: () => <AgendaModule />,
  },
  {
    id: "tasks",
    path: "/tasks",
    label: strings.moduleTasks,
    Icon: ListChecks,
    enabled: true,
    element: () => <TasksModule />,
  },
];

/** Tenant admin (users, domains, DKIM, security). Present in every product. */
export const adminConsole: ProductConsole = {
  path: "/admin/*",
  element: () => <AdminConsole />,
  menu: { to: "/admin", label: strings.adminOpen, Icon: Shield, requires: "admin" },
};

export const defaultPath = "/home";
