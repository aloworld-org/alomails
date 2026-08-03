// The application router. Public routes: sign-in and personal signup.
// Everything else is behind RequireAuth and rendered inside the shell frame;
// the module set comes from the registry, so adding a module is a registry
// entry, not a router change. In alomails, Mail is the surface; the suite
// modules (Docs, Chat, Meet, …) and the multi-tenant control plane live in the
// alo workspace, not here.
import { Fragment } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";

import { useLocale } from "./i18n";
import { AuthProvider, LoginPage, RequireAuth } from "./auth";
import { SignupPage } from "./signup";
import { AppShell, ComingSoon, defaultModulePath, modules } from "./shell";
import { HomeModule } from "./home";
import { MailModule } from "./mail";
import { AdminConsole } from "./admin";

/** The real surface for a module, or a "coming soon" placeholder. */
function moduleElement(id: string, label: string, Icon: (typeof modules)[number]["Icon"]) {
  if (id === "home") return <HomeModule />;
  if (id === "mail") return <MailModule />;
  return <ComingSoon title={label} Icon={Icon} />;
}

export function App() {
  // Subscribe to the active language. Keying the route tree on the
  // locale remounts it on a switch, so every component re-reads
  // `strings.*` in the new language (a rare, deliberate action — the
  // remount cost is invisible and avoids threading a context through
  // ~50 call sites).
  const locale = useLocale();
  return (
    <BrowserRouter>
      <AuthProvider>
        <Fragment key={locale}>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          {/* Public personal signup (ADR 0018); the page hides itself when no
              personal domains are configured. */}
          <Route path="/signup" element={<SignupPage />} />
          {/* The OIDC redirect target; the login flow reads the code inline, so
              a stray navigation here just returns to the app. */}
          <Route path="/auth/callback" element={<Navigate to={defaultModulePath} replace />} />

          <Route element={<RequireAuth />}>
            {/* The admin console has its own full-screen shell (not the mail
                rail); it gates to tenant admins internally. */}
            <Route path="/admin/*" element={<AdminConsole />} />
            <Route element={<AppShell />}>
              <Route index element={<Navigate to={defaultModulePath} replace />} />
              {modules.map((m) => (
                <Route
                  key={m.id}
                  path={`${m.path}/*`}
                  element={moduleElement(m.id, m.label, m.Icon)}
                />
              ))}
              <Route path="*" element={<Navigate to={defaultModulePath} replace />} />
            </Route>
          </Route>
        </Routes>
        </Fragment>
      </AuthProvider>
    </BrowserRouter>
  );
}
