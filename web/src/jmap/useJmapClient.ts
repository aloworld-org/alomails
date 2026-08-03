// Provides a JMAP client bound to the current session's authorized fetch.
// Memoized per auth context so the session cache is shared across the app.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { JmapClient } from "./client";

export function useJmapClient(): JmapClient {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new JmapClient(authorizedFetch), [authorizedFetch]);
}
