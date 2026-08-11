// The application router. Public routes: sign-in and personal signup.
// Everything else is behind RequireAuth and rendered inside the shell frame.
// The whole module/console set comes from the active product surface (ADR
// 0019), so the router is product-agnostic — trimming a product to a subset
// swaps the surface, never this file.
import { Fragment } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";

import { useLocale } from "./i18n";
import {
  AuthProvider,
  ForgotPasswordPage,
  LoginPage,
  RequireAuth,
} from "./auth";
import { SignupPage } from "./signup";
import {
  AppShell,
  ComingSoon,
  ModuleSwitchedOff,
  isModuleAllowed,
  useDeniedModules,
} from "./shell";
import { surface } from "./product";
import type { ProductModule } from "./product";
import { DialogProvider } from "./ds";
import { SiteInvitationView } from "./sites/SiteInvitationView";

/**
 * One module's route, with its per-user switch honoured (migration 0208).
 *
 * The rail already leaves a switched-off app out, so this is reached by a
 * typed URL, an old bookmark or a colleague's link. Rendering the module
 * anyway would mount a screen whose every request answers 403 — a wall of
 * error states instead of a sentence.
 *
 * It waits rather than guessing while the answer is unknown. The alternative,
 * rendering the module and swapping it for the notice a moment later, means
 * firing the module's requests before finding out they were not wanted.
 */
function ModuleRoute({ module }: { module: ProductModule }) {
  const denied = useDeniedModules();
  if (denied === null) return null;
  if (!isModuleAllowed(denied, module.id)) {
    return <ModuleSwitchedOff />;
  }
  return module.element ? (
    module.element()
  ) : (
    <ComingSoon title={module.label} Icon={module.Icon} />
  );
}

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
              {/* Accepting a collaborator invitation. Public by design: the
                  token is the credential, and the person holding it may not
                  have an account yet. */}
              <Route
                path="/sites/invite/:token"
                element={<SiteInvitationView />}
              />
              {/* The OIDC redirect target; the login flow reads the code inline, so
                a stray navigation here just returns to the app. */}
              <Route
                path="/auth/callback"
                element={<Navigate to={surface.defaultPath} replace />}
              />

              <Route element={<RequireAuth />}>
                {/* Full-screen consoles (own shell, gated internally) — e.g. tenant
                  admin, and the control plane in the workspace product. */}
                {surface.consoles.map((c) => (
                  <Route key={c.path} path={c.path} element={c.element()} />
                ))}
                <Route element={<AppShell />}>
                  <Route
                    index
                    element={<Navigate to={surface.defaultPath} replace />}
                  />
                  {surface.modules.map((m) => (
                    <Route
                      key={m.id}
                      path={`${m.path}/*`}
                      element={<ModuleRoute module={m} />}
                    />
                  ))}
                  <Route
                    path="*"
                    element={<Navigate to={surface.defaultPath} replace />}
                  />
                </Route>
              </Route>
            </Routes>
          </Fragment>
        </DialogProvider>
      </AuthProvider>
    </BrowserRouter>
  );
}
