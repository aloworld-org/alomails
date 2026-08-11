// Surface pieces common to every product built on the mail core: the Home and
// Mail modules and the tenant-admin console. No suite-only imports live here,
// so this file (and everything it pulls in) ships in alomails unchanged. Drive
// is NOT here — it is its own product (alodrives), present in the workspace
// surface, not inside the mail app.
import { lazy, Suspense, type ComponentType } from "react";
import { Calendar, Home, ListChecks, Mail, Shield } from "lucide-react";

import { strings } from "../i18n";
import type { ProductConsole, ProductModule } from "./types";

const HomeModule = lazy(() => import("../home").then((m) => ({ default: m.HomeModule })));
const MailModule = lazy(() => import("../mail").then((m) => ({ default: m.MailModule })));
const AgendaModule = lazy(() => import("../agenda").then((m) => ({ default: m.AgendaModule })));
const TasksModule = lazy(() => import("../tasks").then((m) => ({ default: m.TasksModule })));
const AdminConsole = lazy(() => import("../admin").then((m) => ({ default: m.AdminConsole })));

const deferred = (Component: ComponentType) => () => (
  <Suspense fallback={null}>
    <Component />
  </Suspense>
);

export const sharedModules: ProductModule[] = [
  {
    id: "home",
    path: "/home",
    label: strings.moduleHome,
    Icon: Home,
    enabled: true,
    element: deferred(HomeModule),
  },
  {
    id: "mail",
    path: "/mail",
    label: strings.moduleMail,
    Icon: Mail,
    enabled: true,
    element: deferred(MailModule),
  },
  {
    id: "agenda",
    path: "/agenda",
    label: strings.moduleAgenda,
    Icon: Calendar,
    enabled: true,
    element: deferred(AgendaModule),
  },
  {
    id: "tasks",
    path: "/tasks",
    label: strings.moduleTasks,
    Icon: ListChecks,
    enabled: true,
    element: deferred(TasksModule),
  },
];

/** Tenant admin (users, domains, DKIM, security). Present in every product. */
export const adminConsole: ProductConsole = {
  path: "/admin/*",
  element: deferred(AdminConsole),
  menu: { to: "/admin", label: strings.adminOpen, Icon: Shield, requires: "admin" },
};

export const defaultPath = "/home";
