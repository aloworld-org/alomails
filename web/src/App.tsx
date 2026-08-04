// The application router. Public routes: sign-in and personal signup.
// Everything else is behind RequireAuth and rendered inside the shell frame.
// The whole module/console set comes from the active product surface (ADR
// 0019), so the router is product-agnostic — trimming a product to a subset
// swaps the surface, never this file.
import { Fragment } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";

import { useLocale } from "./i18n";
import { AuthProvider, ForgotPasswordPage, LoginPage, RequireAuth } from "./auth";
import { SignupPage } from "./signup";
import { AppShell, ComingSoon } from "./shell";
import { surface } from "./product";
import { DialogProvider } from "./ds";

export function App() {
  // Subscribe to the active language. Keying the route tree on the locale
  // remounts it on a switch, so every component re-reads `strings.*` in the new
  // language (a rare, deliberate action — the remount cost is invisible and
  // avoids threading a context through ~50 call sites).
  const locale = useLocale();
  return (
    <BrowserRouter>
      <AuthProvider>
        <DialogProvider>
        <Fragment key={locale}>
          <Routes>
            <Route path="/login" element={<LoginPage />} />
            {/* Public personal signup (ADR 0018); the page hides itself when no
                personal domains are configured. */}
            <Route path="/signup" element={<SignupPage />} />
            <Route path="/reset" element={<ForgotPasswordPage />} />
            {/* The OIDC redirect target; the login flow reads the code inline, so
                a stray navigation here just returns to the app. */}
            <Route path="/auth/callback" element={<Navigate to={surface.defaultPath} replace />} />

            <Route element={<RequireAuth />}>
              {/* Full-screen consoles (own shell, gated internally) — e.g. tenant
                  admin, and the control plane in the workspace product. */}
              {surface.consoles.map((c) => (
                <Route key={c.path} path={c.path} element={c.element()} />
              ))}
              <Route element={<AppShell />}>
                <Route index element={<Navigate to={surface.defaultPath} replace />} />
                {surface.modules.map((m) => (
                  <Route
                    key={m.id}
                    path={`${m.path}/*`}
                    element={
                      m.element ? m.element() : <ComingSoon title={m.label} Icon={m.Icon} />
                    }
                  />
                ))}
                <Route path="*" element={<Navigate to={surface.defaultPath} replace />} />
              </Route>
            </Route>
          </Routes>
        </Fragment>
        </DialogProvider>
      </AuthProvider>
    </BrowserRouter>
  );
}
