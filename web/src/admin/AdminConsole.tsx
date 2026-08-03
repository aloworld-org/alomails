// The Tenant Admin console shell: a full-screen surface with its own left nav,
// gated to tenant admins. Only the pages with a real backend are in the nav
// today (AI providers); more are added as their backends land — no dead links.
import { useEffect, useState } from "react";
import { Link, Navigate, NavLink, Route, Routes } from "react-router-dom";
import {
  ArrowLeft,
  Globe,
  LayoutDashboard,
  ScrollText,
  ShieldCheck,
  ShieldOff,
  Sparkles,
  UserRound,
  Users,
} from "lucide-react";

import { strings } from "../i18n";
import { Spinner, cx } from "../ds";
import { useJmapClient } from "../jmap";
import { AiProvidersPage } from "./AiProvidersPage";
import { AuditPage } from "./AuditPage";
import { DomainsPage } from "./DomainsPage";
import { GroupsPage } from "./GroupsPage";
import { OverviewPage } from "./OverviewPage";
import { SecurityPage } from "./SecurityPage";
import { UsersPage } from "./UsersPage";
import styles from "./admin.module.css";

export function AdminConsole() {
  const client = useJmapClient();
  const [status, setStatus] = useState<"loading" | "admin" | "denied">("loading");

  useEffect(() => {
    let live = true;
    client
      .isAdmin()
      .then((ok) => {
        if (live) setStatus(ok ? "admin" : "denied");
      })
      .catch(() => {
        if (live) setStatus("denied");
      });
    return () => {
      live = false;
    };
  }, [client]);

  if (status === "loading") {
    return (
      <div className={styles.gate}>
        <Spinner size={24} />
      </div>
    );
  }
  if (status === "denied") {
    return (
      <div className={styles.gate}>
        <div className={styles.denied}>
          <span className={styles.deniedIcon}>
            <ShieldOff size={28} strokeWidth={1.75} />
          </span>
          <h1>{strings.adminDeniedTitle}</h1>
          <p>{strings.adminDeniedBody}</p>
          <Link to="/mail" className={styles.deniedBtn}>
            <ArrowLeft size={16} />
            <span>{strings.adminBackToalo}</span>
          </Link>
        </div>
      </div>
    );
  }

  return (
    <div className={styles.console}>
      <aside className={styles.sidebar}>
        <Link to="/mail" className={styles.back}>
          <ArrowLeft size={16} />
          <span>{strings.adminBackToalo}</span>
        </Link>
        <div className={styles.brand}>{strings.adminTitle}</div>
        <nav className={styles.sideNav}>
          <NavLink
            to="/admin/overview"
            className={({ isActive }) => cx(styles.navItem, isActive && styles.navActive)}
          >
            <LayoutDashboard size={17} strokeWidth={1.75} />
            <span>{strings.adminOverview}</span>
          </NavLink>
          <NavLink
            to="/admin/users"
            className={({ isActive }) => cx(styles.navItem, isActive && styles.navActive)}
          >
            <UserRound size={17} strokeWidth={1.75} />
            <span>{strings.adminUsers}</span>
          </NavLink>
          <NavLink
            to="/admin/groups"
            className={({ isActive }) => cx(styles.navItem, isActive && styles.navActive)}
          >
            <Users size={17} strokeWidth={1.75} />
            <span>{strings.adminGroups}</span>
          </NavLink>
          <NavLink
            to="/admin/domains"
            className={({ isActive }) => cx(styles.navItem, isActive && styles.navActive)}
          >
            <Globe size={17} strokeWidth={1.75} />
            <span>{strings.adminDomains}</span>
          </NavLink>
          <NavLink
            to="/admin/security"
            className={({ isActive }) => cx(styles.navItem, isActive && styles.navActive)}
          >
            <ShieldCheck size={17} strokeWidth={1.75} />
            <span>{strings.adminSecurity}</span>
          </NavLink>
          <NavLink
            to="/admin/ai"
            className={({ isActive }) => cx(styles.navItem, isActive && styles.navActive)}
          >
            <Sparkles size={17} strokeWidth={1.75} />
            <span>{strings.adminAiProviders}</span>
          </NavLink>
          <NavLink
            to="/admin/audit"
            className={({ isActive }) => cx(styles.navItem, isActive && styles.navActive)}
          >
            <ScrollText size={17} strokeWidth={1.75} />
            <span>{strings.adminAudit}</span>
          </NavLink>
        </nav>
      </aside>
      <main className={styles.content}>
        <Routes>
          <Route index element={<Navigate to="/admin/overview" replace />} />
          <Route path="overview" element={<OverviewPage />} />
          <Route path="users" element={<UsersPage />} />
          <Route path="groups" element={<GroupsPage />} />
          <Route path="domains" element={<DomainsPage />} />
          <Route path="security" element={<SecurityPage />} />
          <Route path="ai" element={<AiProvidersPage />} />
          <Route path="audit" element={<AuditPage />} />
          <Route path="*" element={<Navigate to="/admin/overview" replace />} />
        </Routes>
      </main>
    </div>
  );
}
