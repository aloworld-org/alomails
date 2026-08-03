// Route guard: renders the app only for an authenticated session. While the
// session is being restored it shows a centered spinner; with no session it
// redirects to the login screen, remembering where the user was headed.
import { Navigate, Outlet, useLocation } from "react-router-dom";

import { Spinner } from "../ds";
import { strings } from "../i18n";
import { useAuth } from "./AuthProvider";
import styles from "./RequireAuth.module.css";

export function RequireAuth() {
  const { status } = useAuth();
  const location = useLocation();

  if (status === "loading") {
    return (
      <div className={styles.center}>
        <Spinner size={28} label={strings.mailLoading} />
      </div>
    );
  }
  if (status === "anonymous") {
    return <Navigate to="/login" replace state={{ from: location.pathname }} />;
  }
  return <Outlet />;
}
