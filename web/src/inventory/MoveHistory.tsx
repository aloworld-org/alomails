// One product's movements at one place — the screen that makes the model
// comprehensible (B5.09a).
//
// Every row says the same five things: **from → to, how many, why, and which
// document**. That is the whole answer to "where did the other four go", and it
// is why the stock screen has no editable quantity: a person who has read this
// once knows that a number changes here by something *happening*, and asks for
// the receipt or the adjustment instead of for a field.
//
// It reads and writes nothing. The two doors that record a movement are a
// document (a receipt, a delivery, an applied count) and the manual adjustment
// — neither of them is here, and a dialog that only reads is allowed to be only
// a dialog.
import { useEffect, useState } from "react";
import { History, X } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { inventoryMessage, useInventoryApi } from "./api";
import { adjustReasonLabel, momentLabel, moveReasonLabel, qtyLabel } from "./format";
import { ErrorBanner } from "./parts";
import type { InvMove } from "./types";
import styles from "./InventoryModule.module.css";

interface Props {
  productId: string;
  productName: string;
  /** The place the row was opened from; the history is filtered to movements
   *  that touched it, at **either end**. */
  locationId: string;
  locationLabel: string;
  onClose: () => void;
}

export function MoveHistory({
  productId,
  productName,
  locationId,
  locationLabel,
  onClose,
}: Props) {
  const api = useInventoryApi();
  const [moves, setMoves] = useState<InvMove[]>([]);
  /** How many rows the server was willing to send. A page that was filled to
   *  its cap is a page with more behind it, and saying so is the difference
   *  between a short history and a truncated one. */
  const [limit, setLimit] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const read = await api.moves({ productId, locationId });
        if (!live) return;
        setMoves(read.moves);
        setLimit(read.limit);
        setError(null);
      } catch (err) {
        if (live) setError(inventoryMessage(err, strings.inventoryHistoryFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, productId, locationId]);

  return (
    <div className={styles.scrim} role="presentation" onMouseDown={onClose}>
      <div
        className={styles.modalWide}
        role="dialog"
        aria-modal="true"
        aria-label={strings.inventoryHistoryTitle(productName)}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      >
        <div className={styles.modalHead}>
          <span className={styles.modalIcon} aria-hidden="true">
            <History size={19} />
          </span>
          <div className={styles.modalHeadText}>
            <h2>{strings.inventoryHistoryTitle(productName)}</h2>
            <p>{strings.inventoryHistorySubtitle(locationLabel)}</p>
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
          {loading && <Spinner size={16} />}

          {!loading && moves.length === 0 && error === null && (
            <p className={styles.noMatches}>{strings.inventoryHistoryEmpty}</p>
          )}

          {moves.length > 0 && (
            <div className={styles.tableWrap} data-allow-overflow="">
              <table className={styles.table}>
                <thead>
                  <tr>
                    <th scope="col">{strings.inventoryColWhen}</th>
                    <th scope="col">{strings.inventoryColMovement}</th>
                    <th scope="col" className={styles.numeric}>
                      {strings.inventoryColQuantity}
                    </th>
                    <th scope="col">{strings.inventoryColWhy}</th>
                    <th scope="col">{strings.inventoryColDocument}</th>
                  </tr>
                </thead>
                <tbody>
                  {moves.map((move) => (
                    <tr key={move.id}>
                      <td className={styles.muted}>{momentLabel(move.occurredAt)}</td>
                      <td>
                        {/* The direction, in the words of the two ends rather
                            than as a sign: "MAIN → VAN1" is read the same way
                            by everybody, and "−4" is not. */}
                        {move.fromCode} → {move.toCode}
                        <span className={styles.subtle}>
                          {move.fromName} → {move.toName}
                        </span>
                      </td>
                      <td className={styles.numeric}>{qtyLabel(move.qtyMilli)}</td>
                      <td>
                        {moveReasonLabel(move.reason)}
                        {move.reasonCode !== null && (
                          <span className={styles.subtle}>
                            {adjustReasonLabel(move.reasonCode)}
                          </span>
                        )}
                        {move.note !== null && move.note !== "" && (
                          <span className={styles.subtle}>{move.note}</span>
                        )}
                      </td>
                      <td className={styles.muted}>
                        {move.refKind === null ? (
                          strings.inventoryNoDocument
                        ) : (
                          <>
                            {move.refKind}
                            {move.refId !== null && (
                              <span className={styles.subtle}>{move.refId}</span>
                            )}
                          </>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {/* Said only when it is true: a page filled to the server's cap has
              older movements behind it, and a person reading a warehouse's
              history must not take a truncated list for the whole story. */}
          {limit > 0 && moves.length >= limit && (
            <p className={styles.notice}>{strings.inventoryHistoryCapped(limit)}</p>
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
