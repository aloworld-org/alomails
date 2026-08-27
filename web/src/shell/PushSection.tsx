// Web Push notifications (mail M5.3) — the Settings surface for the
// per-device opt-in. The toggle is the browser's own subscription for THIS
// device; the list below it is every device the account registered, each
// removable on its own. Payloads never carry message content (the server's
// contract), so the only privacy decision here is the browser permission —
// which the person makes in the browser's own prompt, never silently.
import { useCallback, useEffect, useState } from "react";
import { X } from "lucide-react";

import { strings } from "../i18n";
import { Button, IconButton, Spinner } from "../ds";
import { useJmapClient } from "../jmap";
import type { PushSettings } from "../jmap";
import {
  currentSubscription,
  pushSupported,
  subscribeThisDevice,
  unsubscribeThisDevice,
} from "../pwa/push";
import styles from "./PushSection.module.css";

/** A device row's readable name: the push service's host — the closest
 * thing to a device label a subscription carries. */
function deviceName(endpoint: string): string {
  try {
    return new URL(endpoint).host;
  } catch {
    return endpoint;
  }
}

export function PushSection() {
  const client = useJmapClient();
  const [settings, setSettings] = useState<PushSettings | null>(null);
  const [localEndpoint, setLocalEndpoint] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [blocked, setBlocked] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const supported = pushSupported();

  const load = useCallback(() => {
    void client
      .pushSettings()
      .then(setSettings)
      .catch(() => setError(strings.pushLoadError));
    if (pushSupported()) {
      void currentSubscription()
        .then((sub) => setLocalEndpoint(sub?.endpoint ?? null))
        .catch(() => setLocalEndpoint(null));
    }
  }, [client]);
  useEffect(load, [load]);

  const thisDeviceOn =
    localEndpoint !== null &&
    settings !== null &&
    settings.subscriptions.some((s) => s.endpoint === localEndpoint);

  async function enable() {
    if (settings?.publicKey == null || busy) return;
    setBusy(true);
    setError(null);
    setBlocked(false);
    try {
      const subscription = await subscribeThisDevice(settings.publicKey);
      if (subscription === null) {
        // The person said no in the browser's prompt — a choice to respect,
        // with a pointer for when it was the browser remembering an old no.
        setBlocked(true);
        return;
      }
      await client.createPushSubscription(subscription);
      load();
    } catch {
      setError(strings.pushError);
    } finally {
      setBusy(false);
    }
  }

  async function disable() {
    if (busy || settings === null) return;
    setBusy(true);
    setError(null);
    try {
      const endpoint = await unsubscribeThisDevice();
      const record = settings.subscriptions.find(
        (s) => s.endpoint === endpoint,
      );
      if (record) await client.deletePushSubscription(record.id);
      load();
    } catch {
      setError(strings.pushError);
    } finally {
      setBusy(false);
    }
  }

  async function remove(id: string) {
    setBusy(true);
    setError(null);
    try {
      await client.deletePushSubscription(id);
      // Removing THIS device's row also lets the browser subscription go,
      // so the toggle reads "off" rather than half-on.
      const removed = settings?.subscriptions.find((s) => s.id === id);
      if (removed && removed.endpoint === localEndpoint) {
        await unsubscribeThisDevice();
        setLocalEndpoint(null);
      }
      load();
    } catch {
      setError(strings.pushError);
    } finally {
      setBusy(false);
    }
  }

  if (settings === null) {
    return error === null ? (
      <Spinner size={18} />
    ) : (
      <p className={styles.error} role="alert">
        {error}
      </p>
    );
  }

  return (
    <div className={styles.wrap}>
      {!settings.enabled ? (
        <p className={styles.none}>{strings.pushNotAvailable}</p>
      ) : !supported ? (
        <p className={styles.none}>{strings.pushUnsupported}</p>
      ) : (
        <div className={styles.deviceCard}>
          <span className={styles.deviceText}>
            <span className={styles.deviceTitle}>
              {strings.pushThisDevice}
            </span>
            <span className={styles.deviceNote}>
              {thisDeviceOn ? strings.pushOnNote : strings.pushOffNote}
            </span>
          </span>
          {thisDeviceOn ? (
            <Button
              variant="ghost"
              onClick={() => void disable()}
              disabled={busy}
            >
              {strings.pushDisable}
            </Button>
          ) : (
            <Button onClick={() => void enable()} disabled={busy}>
              {strings.pushEnable}
            </Button>
          )}
        </div>
      )}

      {blocked && (
        <p className={styles.error} role="alert">
          {strings.pushPermissionBlocked}
        </p>
      )}

      {settings.subscriptions.length > 0 && (
        <ul className={styles.list}>
          {settings.subscriptions.map((s) => (
            <li key={s.id} className={styles.row}>
              <span className={styles.rowName}>{deviceName(s.endpoint)}</span>
              {s.endpoint === localEndpoint && (
                <span className={styles.rowTag}>{strings.pushThisDeviceTag}</span>
              )}
              <span className={styles.rowDates}>
                {strings.pushDeviceSince(
                  new Date(s.createdAt).toLocaleDateString(),
                )}
              </span>
              <IconButton
                label={strings.pushDeviceRemove(deviceName(s.endpoint))}
                icon={<X />}
                onClick={() => void remove(s.id)}
                className={styles.remove}
              />
            </li>
          ))}
        </ul>
      )}

      {settings.enabled && (
        <p className={styles.privacy}>{strings.pushPrivacyNote}</p>
      )}

      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
