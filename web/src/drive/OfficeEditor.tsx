// The office compatibility editor (ADR 0010/0030): a real Word/Excel/PowerPoint
// file (.docx/.xlsx/.pptx/.odt/…) opened in Collabora, embedded same-origin. We
// mint a WOPI token, read Collabora's same-origin discovery to find the editor
// URL, and frame it; Collabora loads and saves the bytes via our /wopi host.
import { useEffect, useState } from "react";
import { X } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { OFFICE_HOST } from "../platform/runtime";
import { driveErrorReason } from "./parts";
import styles from "./OfficeEditor.module.css";

/** File name extensions that open in Collabora. */
export const OFFICE_EXT = /\.(docx?|xlsx?|pptx?|odt|ods|odp|rtf|csv)$/i;

export function OfficeEditor({
  nodeId,
  name,
  onClose,
}: {
  nodeId: string;
  name: string;
  onClose: () => void;
}) {
  const client = useJmapClient();
  const [src, setSrc] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    setSrc(null);
    setError(null);
    void (async () => {
      try {
        const token = await client.driveOfficeToken(nodeId);
        // Discovery is read same-origin (proxied in dev) to avoid CORS.
        const discovery = await (await fetch(`${window.location.origin}/hosting/discovery`)).text();
        const match = discovery.match(/\/browser\/[^"'?]+?\/cool\.html/);
        if (!match) throw new Error(strings.officeDiscoveryMissing);
        // Load the whole editor from OFFICE_HOST so Collabora runs entirely
        // within its own origin (socket, WOPI file, everything) — same-origin in
        // prod, the real backend in local dev. The only cross-origin bit is that
        // the dev app *frames* it, which Collabora's frame_ancestors allows.
        const wopiSrc = `${OFFICE_HOST}/wopi/files/${encodeURIComponent(nodeId)}`;
        const url = `${OFFICE_HOST}${match[0]}?WOPISrc=${encodeURIComponent(wopiSrc)}&access_token=${encodeURIComponent(token)}&lang=en`;
        if (live) setSrc(url);
      } catch (caught) {
        if (live) setError(driveErrorReason(caught) ?? strings.driveUnknownError);
      }
    })();
    return () => {
      live = false;
    };
  }, [attempt, client, nodeId]);

  return (
    <div className={styles.overlay}>
      <header className={styles.head}>
        <button type="button" className={styles.back} onClick={onClose} aria-label={strings.close}>
          <X size={18} />
        </button>
        <span className={styles.name}>{name}</span>
      </header>
      <div className={styles.body}>
        {error !== null ? (
          <div className={styles.officeError} role="alert">
            <h2>{strings.officeUnavailable}</h2>
            <p>{strings.officeLoadFailed(error)}</p>
            <button type="button" onClick={() => setAttempt((value) => value + 1)}>{strings.driveRetry}</button>
          </div>
        ) : src === null ? (
          <OfficeSkeleton />
        ) : (
          <iframe
            title={name}
            src={src}
            className={styles.frame}
            allow="clipboard-read; clipboard-write; fullscreen"
          />
        )}
      </div>
    </div>
  );
}

function OfficeSkeleton() {
  return (
    <div className={styles.officeSkeleton} role="status" aria-label={strings.officeLoading} aria-busy="true">
      <span className={styles.officeSkeletonToolbar} />
      <span className={styles.officeSkeletonPage} />
    </div>
  );
}
