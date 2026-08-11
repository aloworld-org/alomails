// Scanning a barcode (B5.09c) — the screen behind pointing a machine at a box.
//
// **The keyboard-wedge scanner is the headline and the camera is the
// fallback**, which is the opposite of how a demo would order them and the
// right way round for a warehouse (`docs/design/inventory.md` § Web surface).
// A wedge scanner is a keyboard: it types the digits and presses Enter. It
// therefore needs no permission, no HTTPS, no library and no camera, it works
// on the ten-year-old PC bolted to the packing bench, and it is what the
// hardware in the room actually is. The camera is for the phone in somebody's
// hand, where it is genuinely better — and it is offered only where the browser
// can do it, never announced and then refused.
//
// Three decisions this file makes, each of them a way a scanner can lie:
//
// - **The field is cleared on a hit and selected on a miss.** A wedge scanner
//   types into whatever has focus, so text left behind from the last scan would
//   be prefixed to the next one and produce a code that never existed. Clearing
//   handles the hit; selecting handles the miss, where the digits are worth
//   reading before the next scan replaces them.
// - **A misread code and an unknown product are different answers**, and the
//   server is what tells them apart: a code whose check digit fails is a `422`
//   with the rule it broke, a well-formed code nobody stocks is a `404`. A
//   person told "not found" for a code the scanner mangled will search the
//   shelves for a product that was there all along.
// - **The screen states nothing the server did not.** The quantity is the
//   server's fold of the movement ledger, the places are its rows, and a
//   refusal is its sentence — this file's own strings are the fallback for a
//   request that never arrived.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Camera, ScanLine, X } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { InventoryError, inventoryMessage, useInventoryApi } from "./api";
import { qtyLabel } from "./format";
import { ErrorBanner } from "./parts";
import type { ScanResult } from "./types";
import styles from "./InventoryModule.module.css";

/** One barcode the browser's detector read out of a video frame. */
interface DetectedBarcode {
  rawValue: string;
}

/** The slice of the `BarcodeDetector` API this file uses. It is not in the DOM
 *  typings, and declaring the two members we call is honest about how little of
 *  it we depend on. */
interface BarcodeDetectorLike {
  detect(source: CanvasImageSource): Promise<DetectedBarcode[]>;
}

type BarcodeDetectorCtor = new (options?: { formats?: string[] }) => BarcodeDetectorLike;

/**
 * The symbologies a GTIN is printed in.
 *
 * Deliberately **not** `upc_e`: its raw value is the compressed six-digit form,
 * whose check digit belongs to the expanded code, so a UPC-E scan would reach
 * the server as a code that fails validation — a refusal with no cause a person
 * could act on. Expanding it is arithmetic this file is not allowed to do
 * (the barcode rules live in the store, `inv_barcode.rs`), so the format is
 * left out until they can be asked to.
 */
const GTIN_FORMATS = ["ean_13", "ean_8", "upc_a", "itf"];

/** How often a video frame is examined. Fast enough that aiming feels
 *  immediate, slow enough that a phone does not heat up doing it. */
const DETECT_INTERVAL_MS = 400;

/** The detector constructor, when this browser has one. Chrome and Android have
 *  it; Safari and Firefox do not, and the camera is simply not offered there. */
function barcodeDetector(): BarcodeDetectorCtor | null {
  const ctor = (window as unknown as { BarcodeDetector?: BarcodeDetectorCtor }).BarcodeDetector;
  return ctor ?? null;
}

/** What a screen does with the product a scan found, and what it calls it. */
export interface ScanAction {
  label: string;
  run: (found: ScanResult) => void;
}

interface Props {
  onClose: () => void;
  /** The act the scanning screen offers on a hit — opening the product on the
   *  catalog, finding it in the list on the stock screen. Absent means the
   *  answer is the whole point and there is nothing further to do with it. */
  action?: ScanAction;
  /** What to offer when the code was a real GTIN and no product carries it.
   *  On the catalog that is adding one with the code already filled in, which
   *  is the whole of "empty states are onboarding" for a scanner. */
  onUnknown?: { label: string; run: (code: string) => void };
}

export function ScanInput({ onClose, action, onUnknown }: Props) {
  const api = useInventoryApi();
  const inputRef = useRef<HTMLInputElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const [code, setCode] = useState("");
  const [found, setFound] = useState<ScanResult | null>(null);
  /** The well-formed code that matched nothing, which is the only case the
   *  "add it to the catalog" offer makes sense for. A mangled code must never
   *  become a product's barcode. */
  const [unknown, setUnknown] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cameraOn, setCameraOn] = useState(false);
  const [cameraError, setCameraError] = useState<string | null>(null);

  /** Whether this browser can read a code out of a camera at all. Asked once:
   *  a button that appears and then apologises is worse than no button. */
  const cameraPossible = useMemo(
    () => barcodeDetector() !== null && typeof navigator.mediaDevices?.getUserMedia === "function",
    [],
  );

  const lookup = useCallback(
    async (raw: string) => {
      const scanned = raw.trim();
      if (scanned === "") return;
      setBusy(true);
      setError(null);
      try {
        const result = await api.scan(scanned);
        setFound(result);
        setUnknown(null);
        // A hit clears the field, so the next scan starts from nothing.
        setCode("");
      } catch (err) {
        setFound(null);
        setError(inventoryMessage(err, strings.inventoryScanFailed));
        setUnknown(err instanceof InventoryError && err.status === 404 ? scanned : null);
        // A miss keeps the digits and selects them: they are worth reading, and
        // the next scan overwrites them rather than appending to them.
        inputRef.current?.select();
      } finally {
        setBusy(false);
      }
    },
    [api],
  );

  // The field owns the focus for as long as the dialog is open, because a wedge
  // scanner types wherever the focus happens to be — and a scan that lands in
  // the page behind this one is a scan that silently did nothing.
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    if (!cameraOn) return undefined;
    const Detector = barcodeDetector();
    if (Detector === null) return undefined;
    let live = true;
    let stream: MediaStream | null = null;
    let timer: number | undefined;

    const stop = () => {
      if (timer !== undefined) window.clearInterval(timer);
      stream?.getTracks().forEach((track) => track.stop());
    };

    void (async () => {
      try {
        // The back camera on a phone; on a laptop the only camera there is.
        stream = await navigator.mediaDevices.getUserMedia({
          video: { facingMode: "environment" },
        });
        const video = videoRef.current;
        if (!live || video === null) {
          stop();
          return;
        }
        video.srcObject = stream;
        await video.play();
        const detector = new Detector({ formats: GTIN_FORMATS });
        timer = window.setInterval(() => {
          void (async () => {
            const element = videoRef.current;
            if (!live || element === null) return;
            let codes: DetectedBarcode[] = [];
            try {
              codes = await detector.detect(element);
            } catch {
              // A frame the detector could not read is not an error worth
              // showing: the next one is 400 ms away.
              return;
            }
            const first = codes[0]?.rawValue.trim() ?? "";
            if (first === "") return;
            // One reading is the whole act: the camera stops and the code is
            // looked up, rather than the same box being re-read four times a
            // second while somebody reads the answer.
            setCameraOn(false);
            void lookup(first);
          })();
        }, DETECT_INTERVAL_MS);
      } catch {
        // Permission refused, no camera, or a browser that will not hand one
        // over on this origin. The typing path is untouched and still the one
        // a warehouse uses.
        if (live) {
          setCameraError(strings.inventoryScanCameraFailed);
          setCameraOn(false);
        }
      }
    })();

    return () => {
      live = false;
      stop();
    };
  }, [cameraOn, lookup]);

  return (
    <div className={styles.scrim} role="presentation" onMouseDown={onClose}>
      <div
        className={styles.scanModal}
        role="dialog"
        aria-modal="true"
        aria-label={strings.inventoryScanTitle}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      >
        <div className={styles.modalHead}>
          <span className={styles.modalIcon} aria-hidden="true">
            <ScanLine size={19} />
          </span>
          <div className={styles.modalHeadText}>
            <h2>{strings.inventoryScanTitle}</h2>
            <p>{strings.inventoryScanSubtitle}</p>
          </div>
          <button
            type="button"
            className={styles.modalClose}
            onClick={onClose}
            aria-label={strings.inventoryClose}
          >
            <X size={18} />
          </button>
        </div>

        <div className={styles.modalBody}>
          <form
            className={styles.scanForm}
            onSubmit={(e) => {
              e.preventDefault();
              void lookup(code);
            }}
          >
            <label className={styles.field}>
              <span className={styles.fieldLabel}>{strings.inventoryScanFieldCode}</span>
              <input
                ref={inputRef}
                className={styles.input}
                type="text"
                // A numeric keypad on a phone, and never a browser's guess at
                // what a thirteen-digit number "should" be.
                inputMode="numeric"
                autoComplete="off"
                spellCheck={false}
                value={code}
                onChange={(e) => setCode(e.target.value)}
                placeholder={strings.inventoryScanPlaceholder}
              />
              <span className={styles.hint}>{strings.inventoryScanHint}</span>
            </label>
            <Button type="submit" disabled={busy || code.trim() === ""}>
              {strings.inventoryScanLookup}
            </Button>
            {cameraPossible && (
              <Button variant="ghost" onClick={() => setCameraOn(!cameraOn)}>
                <Camera size={16} />{" "}
                {cameraOn ? strings.inventoryScanCameraStop : strings.inventoryScanCameraStart}
              </Button>
            )}
            {busy && <Spinner size={16} />}
          </form>

          {/* Said only where it is true, and never as an apology on a machine
              that has a scanner plugged into it. */}
          {!cameraPossible && <p className={styles.hint}>{strings.inventoryScanNoCamera}</p>}
          {cameraError !== null && <ErrorBanner message={cameraError} />}

          {cameraOn && (
            <div className={styles.scanCamera}>
              {/* Muted and inline, or a phone browser refuses to play it at
                  all; no audio track is requested in the first place. */}
              <video ref={videoRef} className={styles.scanVideo} muted playsInline />
              <p className={styles.hint}>{strings.inventoryScanAiming}</p>
            </div>
          )}

          {error !== null && <ErrorBanner message={error} />}

          {unknown !== null && onUnknown !== undefined && (
            <p className={styles.notice}>
              <Button
                variant="ghost"
                onClick={() => {
                  onUnknown.run(unknown);
                }}
              >
                {onUnknown.label}
              </Button>
            </p>
          )}

          {found !== null && (
            <div className={styles.scanResult}>
              <p className={styles.scanProduct}>
                {found.product.name}
                <span className={styles.subtle}>
                  {found.product.sku === ""
                    ? found.code
                    : `${found.product.sku} · ${found.code}`}
                </span>
              </p>

              {found.product.stocked ? (
                <>
                  <p className={styles.scanTotal}>
                    {strings.inventoryScanOnHand(qtyLabel(found.onHandQtyMilli))}
                  </p>
                  {found.stock.length === 0 ? (
                    <p className={styles.muted}>{strings.inventoryScanNowhere}</p>
                  ) : (
                    <ul className={styles.scanPlaces}>
                      {found.stock.map((level) => (
                        <li key={level.locationId} className={styles.scanPlace}>
                          <span>
                            {level.locationCode}
                            <span className={styles.subtle}>{level.locationName}</span>
                          </span>
                          <span className={styles.numeric}>{qtyLabel(level.qtyMilli)}</span>
                        </li>
                      ))}
                    </ul>
                  )}
                </>
              ) : (
                // A service has no shelf and no quantity — not a zero, which
                // would read as an empty one.
                <p className={styles.muted}>{strings.inventoryScanServiceNote}</p>
              )}

              {action !== undefined && (
                <Button
                  onClick={() => {
                    action.run(found);
                  }}
                >
                  {action.label}
                </Button>
              )}
            </div>
          )}

          {found === null && error === null && !busy && (
            <p className={styles.noMatches}>{strings.inventoryScanWaiting}</p>
          )}
        </div>

        <div className={styles.modalFooter}>
          <span className={styles.modalFooterSpacer} />
          <Button variant="ghost" onClick={onClose}>
            {strings.inventoryClose}
          </Button>
        </div>
      </div>
    </div>
  );
}
