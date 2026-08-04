// Silent auto-update for the desktop app; a no-op in the browser.
//
// On launch the app asks the signed update feed whether a newer version is
// published. If so it downloads the package, the Tauri updater verifies its
// minisign signature against the public key baked into the build, installs it,
// and relaunches into the new version. Nothing installs without a signature the
// public key verifies, and any failure is swallowed — an update problem must
// never stop someone using the app they already have.
//
// The plugin modules are imported dynamically so they are only pulled in inside
// the desktop shell; the browser bundle never loads them.
import { inTauri } from "./runtime";

export async function checkForUpdatesInBackground(): Promise<void> {
  if (!inTauri) return;
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (update === null) return;
    await update.downloadAndInstall();
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch (err) {
    // Best-effort: log and carry on with the current version.
    console.warn("alomails: update check failed", err);
  }
}
