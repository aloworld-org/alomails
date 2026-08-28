// Booking a consignment — an arrival against an order we placed, or a
// despatch against one a customer placed (B5.09b).
//
// One dialog for both, because they are one act in two directions: goods move
// between a real place and a counterparty, the order advances, and the ledger
// gains a row per line. The words differ and the endpoint differs; the sheet
// does not.
//
// **It opens on what is still owing.** Every line starts filled in with its
// outstanding quantity, so the ordinary case — the delivery that matches the
// order — is one click, and the exceptional case is a number typed over. A
// short delivery is then something a person *did*, not something they forgot.
//
// **It refuses nothing that the server rules on.** Whether a quantity exceeds
// what is outstanding, whether an order is in a state that can take a
// consignment, whether the place is one of this tenant's — all of that is
// decided under the row lock that books the movement, and shown here in the
// server's own sentence. What this file checks is only what it can know alone:
// that the text typed into a quantity box is a quantity, and that the sheet
// says *something*, because an empty sheet is refused and asking first is
// kinder than a round trip.
import { useState } from "react";
import { PackageCheck, X } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { inventoryMessage } from "./api";
import { qtyLabel } from "./format";
import { fulfilDraft, type FulfilRow } from "./orderRows";
import { ErrorBanner, Field } from "./parts";
import type { FulfilmentDraft, InvLocation } from "./types";
import { milliToInput } from "../billing";
import styles from "./InventoryModule.module.css";

/** One line of the order, as the sheet needs it. */
export interface FulfilLine {
  lineId: string;
  description: string;
  unit: string;
  /** What is still to come, or still to go. `0` for a charge in words, which
   *  no consignment carries. */
  outstandingQtyMilli: number;
}

interface Props {
  title: string;
  subtitle: string;
  /** What the place column is called here: where the goods were *put*, or
   *  where they were picked *from*. */
  locationLabel: string;
  locationHint: string;
  submitLabel: string;
  /** The tenant's real places. The four counterparties are never offered: a
   *  consignment's other end is the document's, not a choice. */
  locations: InvLocation[];
  lines: FulfilLine[];
  onSubmit: (draft: FulfilmentDraft) => Promise<void>;
  onClose: () => void;
}

export function FulfilDialog({
  title,
  subtitle,
  locationLabel,
  locationHint,
  submitLabel,
  locations,
  lines,
  onSubmit,
  onClose,
}: Props) {
  const [locationId, setLocationId] = useState(locations[0]?.id ?? "");
  const [rows, setRows] = useState<FulfilRow[]>(() =>
    lines.map((line) => ({
      lineId: line.lineId,
      // Opening on what is owed is the whole ergonomics of this sheet; a line
      // that cannot move opens blank rather than at zero, which would read as a
      // decision somebody made.
      qty: line.outstandingQtyMilli > 0 ? milliToInput(line.outstandingQtyMilli) : "",
    })),
  );
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function setQty(lineId: string, qty: string) {
    setRows((current) => current.map((row) => (row.lineId === lineId ? { ...row, qty } : row)));
  }

  async function submit() {
    if (locationId === "") {
      setError(strings.inventoryFulfilNeedsPlace);
      return;
    }
    const stated = fulfilDraft(rows);
    if (stated === null) {
      setError(strings.inventoryNotAQuantity);
      return;
    }
    if (stated.length === 0) {
      setError(strings.inventoryFulfilNeedsSomething);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onSubmit({
        locationId,
        lines: stated,
        ...(note.trim() === "" ? {} : { note: note.trim() }),
      });
    } catch (err) {
      // The dialog stays open with what was typed still in it: a refusal is
      // something to correct, not a form to fill in again.
      setError(inventoryMessage(err, strings.inventorySaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={styles.scrim} role="presentation" onMouseDown={onClose}>
      <div
        className={styles.modalWide}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      >
        <div className={styles.modalHead}>
          <span className={styles.modalIcon} aria-hidden="true">
            <PackageCheck size={19} />
          </span>
          <div className={styles.modalHeadText}>
            <h2>{title}</h2>
            <p>{subtitle}</p>
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
          {error !== null && <ErrorBanner message={error} />}

          <Field label={locationLabel} hint={locationHint}>
            <select
              className={styles.select}
              value={locationId}
              onChange={(e) => setLocationId(e.target.value)}
            >
              {locations.length === 0 && <option value="">{strings.inventoryNoPlaces}</option>}
              {locations.map((place) => (
                <option key={place.id} value={place.id}>
                  {place.code} — {place.name}
                </option>
              ))}
            </select>
          </Field>

          <div className={styles.tableWrap} data-allow-overflow="">
            <table className={styles.table}>
              <thead>
                <tr>
                  <th scope="col">{strings.inventoryColDescription}</th>
                  <th scope="col" className={styles.numeric}>
                    {strings.inventoryColOutstanding}
                  </th>
                  <th scope="col" className={styles.numeric}>
                    {strings.inventoryColThisConsignment}
                  </th>
                </tr>
              </thead>
              <tbody>
                {lines.map((line) => {
                  const row = rows.find((r) => r.lineId === line.lineId);
                  return (
                    <tr key={line.lineId}>
                      <td>
                        {line.description}
                        {line.unit !== "" && <span className={styles.subtle}>{line.unit}</span>}
                      </td>
                      <td className={styles.numeric}>
                        {line.outstandingQtyMilli > 0 ? (
                          qtyLabel(line.outstandingQtyMilli)
                        ) : (
                          <span className={styles.muted}>{strings.inventoryNotStocked}</span>
                        )}
                      </td>
                      <td className={styles.numeric}>
                        {line.outstandingQtyMilli > 0 ? (
                          <input
                            className={`${styles.input} ${styles.inputNarrow} ${styles.numeric}`}
                            value={row?.qty ?? ""}
                            onChange={(e) => setQty(line.lineId, e.target.value)}
                            inputMode="decimal"
                            aria-label={`${strings.inventoryColThisConsignment} — ${line.description}`}
                          />
                        ) : (
                          // A charge in words has nothing to arrive; a box here
                          // would invite a number the server must refuse.
                          <span className={styles.muted}>{strings.inventoryNotStocked}</span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>

          <Field label={strings.inventoryFieldNote} hint={strings.inventoryFulfilNoteHint}>
            <textarea
              className={`${styles.input} ${styles.textarea}`}
              value={note}
              onChange={(e) => setNote(e.target.value)}
              rows={2}
            />
          </Field>
        </div>

        <div className={styles.modalFooter}>
          {busy && <Spinner size={16} />}
          <span className={styles.modalFooterSpacer} />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.inventoryCancelAction}
          </Button>
          <Button onClick={() => void submit()} disabled={busy}>
            {submitLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
