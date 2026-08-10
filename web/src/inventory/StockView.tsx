// Stock: what is where, right now (B5.09a).
//
// **There is no editable quantity on this screen, and its absence is the
// design.** On-hand is derived from the movement ledger the way a balance is
// derived from postings (`docs/design/inventory.md` § Locations and the move
// ledger); a number typed in here could not answer the question a warehouse
// actually asks, which is *where did the other four go*. The answer to that
// question is one click away on every row — the history — and a person who
// reads it once stops looking for the field.
//
// **The virtual counterparties are off by default.** `supplier` holding minus
// four hundred is an accounting fact about how much has come from outside, not
// a shelf; showing it by default would make every total on this screen wrong.
// The toggle exists for the person checking that the ledger closes, and the
// screen says what the total means when it is on.
//
// **The value column is a reference figure, not a balance.** B5 chooses no
// costing method and posts nothing to the journal, so this is what the goods
// cost us at today's purchase price and the screen says so where it is shown
// rather than in a manual nobody opens.
import { useCallback, useEffect, useMemo, useState } from "react";
import { Boxes } from "lucide-react";

import { Spinner } from "../ds";
import { strings } from "../i18n";
import { inventoryMessage, useInventoryApi } from "./api";
import { MoveHistory } from "./MoveHistory";
import { locationKindLabel, momentLabel, qtyLabel, valueLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import type { InvLocation, StockLevel } from "./types";
import styles from "./InventoryModule.module.css";

/** Which product's history is open, and at which place it was opened from —
 *  the history is filtered to that place, because "what happened to this row"
 *  is the question the row was clicked to ask. */
interface Opened {
  level: StockLevel;
}

export function StockView() {
  const api = useInventoryApi();
  const [locations, setLocations] = useState<InvLocation[]>([]);
  const [levels, setLevels] = useState<StockLevel[]>([]);
  const [totalValueCents, setTotalValueCents] = useState(0);
  const [location, setLocation] = useState("");
  const [includeVirtual, setIncludeVirtual] = useState(false);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [opened, setOpened] = useState<Opened | null>(null);
  const [revision, setRevision] = useState(0);

  const reload = useCallback(() => setRevision((r) => r + 1), []);

  // The places, once. This read is also what seeds a tenant's starting set of
  // locations, so it runs even when the stock read answers nothing.
  useEffect(() => {
    let live = true;
    void api
      .locations()
      .then((places) => {
        if (live) setLocations(places);
      })
      .catch((err: unknown) => {
        if (live) setError(inventoryMessage(err, strings.inventoryLoadFailed));
      });
    return () => {
      live = false;
    };
  }, [api]);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const read = await api.stock({
          ...(location === "" ? {} : { locationId: location }),
          includeVirtual,
        });
        if (!live) return;
        setLevels(read.stock);
        // The server's sum of exactly the rows it sent — never re-added here,
        // so a filtered screen and its total can never disagree.
        setTotalValueCents(read.totalValueCents);
        setError(null);
      } catch (err) {
        if (live) setError(inventoryMessage(err, strings.inventoryLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, location, includeVirtual, revision]);

  /** The real places, for the filter. The virtual four are never offered as a
   *  place to look at: a person filtering by "customer" would be reading the
   *  history of everything ever delivered, which is the ledger's screen. */
  const places = useMemo(
    () => locations.filter((place) => !place.system && !place.archived),
    [locations],
  );

  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (needle === "") return levels;
    return levels.filter((level) =>
      `${level.productName} ${level.sku} ${level.locationCode} ${level.locationName}`
        .toLowerCase()
        .includes(needle),
    );
  }, [levels, search]);

  return (
    <div className={styles.page}>
      <div className={styles.toolbar}>
        <input
          className={styles.search}
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.inventorySearchStock}
          aria-label={strings.inventorySearchStock}
        />
        <label className={styles.filterField}>
          {strings.inventoryFilterLocation}
          <select
            className={styles.select}
            value={location}
            onChange={(e) => setLocation(e.target.value)}
          >
            <option value="">{strings.inventoryAllLocations}</option>
            {places.map((place) => (
              <option key={place.id} value={place.id}>
                {place.code} — {place.name}
              </option>
            ))}
          </select>
        </label>
        <label className={styles.toggle}>
          <input
            type="checkbox"
            checked={includeVirtual}
            onChange={(e) => setIncludeVirtual(e.target.checked)}
          />
          {strings.inventoryShowCounterparties}
        </label>
        <span className={styles.toolbarSpacer} />
        {loading && <Spinner size={16} />}
      </div>

      {error !== null && <ErrorBanner message={error} />}

      {/* What the counterparties do to the total, said where the total is —
          with them in, a closed ledger sums to roughly nothing, and a reader
          who was not told that reads it as an empty warehouse. */}
      {includeVirtual && <p className={styles.notice}>{strings.inventoryCounterpartiesNote}</p>}

      {levels.length === 0 && !loading ? (
        <EmptyState
          Icon={Boxes}
          title={strings.inventoryStockEmptyTitle}
          body={strings.inventoryStockEmptyBody}
        />
      ) : shown.length === 0 && !loading ? (
        <p className={styles.noMatches}>{strings.inventoryNoMatches}</p>
      ) : (
        <>
          <div className={styles.tableWrap}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th scope="col">{strings.inventoryColProduct}</th>
                  <th scope="col">{strings.inventoryColLocation}</th>
                  <th scope="col" className={styles.numeric}>
                    {strings.inventoryColOnHand}
                  </th>
                  <th scope="col" className={styles.numeric}>
                    {strings.inventoryColValue}
                  </th>
                  <th scope="col">{strings.inventoryColLastMove}</th>
                  <th scope="col">
                    <span className={styles.srOnly}>{strings.inventoryColActions}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {shown.map((level) => (
                  <tr key={`${level.productId}:${level.locationId}`}>
                    <td>
                      {level.productName}
                      {level.sku !== "" && <span className={styles.subtle}>{level.sku}</span>}
                    </td>
                    <td>
                      {level.locationCode}
                      <span className={styles.subtle}>
                        {level.real
                          ? level.locationName
                          : `${level.locationName} · ${locationKindLabel(level.locationKind)}`}
                      </span>
                    </td>
                    <td className={styles.numeric}>{qtyLabel(level.qtyMilli)}</td>
                    <td className={styles.numeric}>{valueLabel(level.valueCents)}</td>
                    <td className={styles.muted}>
                      {level.lastMoveAt === null ? "" : momentLabel(level.lastMoveAt)}
                    </td>
                    <td className={styles.rowActions}>
                      <button
                        type="button"
                        className={styles.linkAction}
                        onClick={() => setOpened({ level })}
                      >
                        {strings.inventoryOpenHistory}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className={styles.totalLine}>
            {strings.inventoryReferenceValue(valueLabel(totalValueCents))}
          </p>
        </>
      )}

      {opened !== null && (
        <MoveHistory
          productId={opened.level.productId}
          productName={opened.level.productName}
          locationId={opened.level.locationId}
          locationLabel={`${opened.level.locationCode} — ${opened.level.locationName}`}
          onClose={() => {
            setOpened(null);
            // A history read cannot change stock, but the ledger it shows may
            // have moved while it was open — a receipt booked by a colleague —
            // so the list behind it is re-read rather than left stale.
            reload();
          }}
        />
      )}
    </div>
  );
}
