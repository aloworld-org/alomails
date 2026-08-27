// Bridges the locale store to the server-synced preference (mail M4.2).
// Mounted once inside AuthProvider; renders nothing. On sign-in it adopts
// the server's stored choice, and while the session lives it registers the
// writer that persists a switcher change — so the same person signs in
// speaking the same language on every device. Anonymous pages never mount
// a writer: there, browser detection stays the whole story.
//
// Imported directly (not through the i18n barrel): this file depends on
// auth and jmap, and the barrel is imported *by* auth — routing it through
// the barrel would close an import cycle for no gain.
import { useEffect } from "react";

import { useAuth } from "../auth";
import { useJmapClient } from "../jmap";
import { adoptRemoteLocale, setRemoteLocaleWriter } from "./locale";

export function LocaleSync() {
  const { status } = useAuth();
  const client = useJmapClient();

  useEffect(() => {
    if (status !== "authenticated") return;
    let live = true;
    void client
      .localePreference()
      .then((locale) => {
        if (live) adoptRemoteLocale(locale);
      })
      .catch(() => {
        // Unreachable or errored → the locally detected locale stands.
      });
    setRemoteLocaleWriter((locale) => {
      void client.setLocalePreference(locale).catch(() => {
        // Best-effort: the switch already applied locally, and the next
        // successful switch will sync it.
      });
    });
    return () => {
      live = false;
      setRemoteLocaleWriter(null);
    };
  }, [status, client]);

  return null;
}
