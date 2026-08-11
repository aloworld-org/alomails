// Which apps this person was given (migration 0208).
//
// A tenant admin switches modules off per user. The session carries the list,
// the rail leaves those entries out, and a typed URL lands on a plain notice
// rather than on a module whose every request answers 403.
//
// **This is not the access control.** `module_access.rs` refuses the routes on
// the server, and would refuse them just the same if this file did not exist.
// What it buys is honesty: offering somebody an app that fails when they click
// it is worse than not offering it, and a person who typed the URL deserves a
// sentence rather than a broken screen.
//
// So the failure direction is deliberate: while the answer is unknown, and if
// the request for it fails, everything is treated as allowed. The worst case
// is an app in the rail that refuses when opened — which is exactly the state
// before this feature existed. Failing the other way would hide somebody's
// whole workspace because one fetch timed out.
import { useEffect, useState } from "react";

import { useJmapClient } from "../jmap";

/** The denied set, or `null` while it is still being read. */
export type DeniedModules = ReadonlySet<string> | null;

/**
 * The modules a tenant admin has switched off for the signed-in user.
 *
 * `null` means "not known yet" and is not the same as "none denied" — a caller
 * that treats them the same will flash a module away after it has drawn it.
 * Empty for an admin, who is never denied.
 */
export function useDeniedModules(): DeniedModules {
  const client = useJmapClient();
  const [denied, setDenied] = useState<DeniedModules>(null);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const session = await client.session();
        if (live) setDenied(new Set(session["alo:deniedModules"] ?? []));
      } catch {
        // Allow everything rather than hide everything. See the file header.
        if (live) setDenied(new Set());
      }
    })();
    return () => {
      live = false;
    };
  }, [client]);

  return denied;
}

/**
 * Whether a module should be offered, given the denied set.
 *
 * Unknown reads as allowed, so the rail draws its usual entries on the first
 * paint and removes the rare denied one when the answer arrives — rather than
 * drawing an empty rail for everybody and filling it in.
 */
export function isModuleAllowed(denied: DeniedModules, id: string): boolean {
  return denied === null || !denied.has(id);
}
