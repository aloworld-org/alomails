// The shop-setup proposal screen (ADR 0041, S3.05b3): describe the business,
// review the proposed catalog with every guess visibly flagged, edit, then
// approve — the screen where a shop gets configured without a consultant.
//
// Three facts shape it, all inherited from the envelope rather than invented
// here:
//
//   * **Every price is stated or blank.** The parser refuses proposals with
//     invented numbers, so a prefilled price on this screen is one the
//     business description itself stated; a blank is a required field the
//     owner types, never a guess.
//   * **VAT is structurally a guess.** The envelope cannot say "confirmed" —
//     each row shows the proposed rate with its one-sentence basis, and the
//     confirmation is the owner clicking Approve after reading it.
//   * **Approving applies through the owned routes only.** Each ticked row
//     becomes one `POST /billing/products` (Billing's own door) and the
//     delivery rate one `PUT .../shop-settings`; there is no bulk-apply
//     endpoint. A row the server refuses fails alone, with the server's
//     sentence on it, and approving again re-sends only what is still
//     pending.
import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Check, Sparkles } from "lucide-react";
import { Link, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { SitesError, sitesMessage, useSitesApi } from "./api";
import { formatPrice, parsePriceInput, priceInput } from "./catalogPricing";
import { ErrorBanner, Field } from "./parts";
import type {
  ShopConfigProposal,
  ShopProposalItem,
  SiteDetail,
  SiteTicketProductList,
} from "./types";

const styles = {
  page: "mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 py-6 sm:px-6 lg:px-8",
  header: "flex flex-wrap items-center gap-4 border-b border-subtle pb-5",
  backLink:
    "inline-flex min-h-10 items-center gap-2 rounded-xl px-3 text-sm font-semibold text-secondary no-underline transition hover:bg-muted hover:text-primary",
  siteHead: "min-w-0 flex-1",
  title: "text-2xl font-semibold tracking-tight text-primary",
  submissionSiteName: "mt-1 block truncate text-sm text-secondary",
  headerActions: "flex min-h-10 items-center",
  hint: "text-sm leading-6 text-secondary [&_a]:font-semibold [&_a]:text-accent [&_a]:no-underline hover:[&_a]:opacity-80",
  shopDescribe:
    "mx-auto flex w-full max-w-3xl flex-col gap-5 rounded-2xl border border-subtle bg-surface p-6 shadow-sm sm:p-8",
  shopIntro: "max-w-2xl text-base leading-7 text-secondary",
  input:
    "min-h-11 w-full rounded-xl border border-default bg-surface px-3.5 py-2.5 text-primary outline-none transition placeholder:text-tertiary focus:border-accent focus:ring-2 focus:ring-accent/15 disabled:cursor-not-allowed disabled:bg-muted disabled:text-tertiary",
  textarea: "min-h-36 resize-y leading-6",
  shopActions: "flex flex-wrap items-center gap-3 pt-1",
  shopProposal: "flex flex-col gap-5",
  sectionTitle: "text-xl font-semibold tracking-tight text-primary",
  shopRows: "grid list-none gap-4 p-0 lg:grid-cols-2",
  shopRow:
    "flex min-w-0 flex-col gap-4 rounded-2xl border border-subtle bg-surface p-5 shadow-sm",
  shopRowHead: "flex flex-wrap items-center gap-2 border-b border-subtle pb-4",
  chip: "inline-flex min-h-7 items-center gap-1.5 rounded-full bg-muted px-2.5 text-xs font-semibold text-secondary",
  chipLive: "bg-success-tint text-success",
  fieldRow: "grid gap-4 sm:grid-cols-2",
  shopGuess:
    "rounded-xl bg-warning/10 px-3.5 py-3 text-sm leading-6 text-secondary",
  badge:
    "mr-1 inline-flex rounded-full bg-surface px-2 py-0.5 text-xs font-semibold text-warning shadow-sm",
  shopShipping:
    "flex flex-col gap-4 rounded-2xl border border-subtle bg-surface p-5 shadow-sm sm:p-6",
  shopShippingTitle: "text-base font-semibold text-primary",
  shopDone:
    "flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-success/25 bg-success-tint p-5 text-sm font-semibold text-primary [&_a]:text-accent [&_a]:no-underline",
  submitRequirement: "text-sm font-medium text-secondary",
};

/** One proposed item as the owner is editing it. `priceStated` remembers
 *  whether the price arrived stated or as a flagged blank — the blank keeps
 *  its "the description didn't say" hint even after something is typed. */
interface ProposalRow {
  key: number;
  included: boolean;
  name: string;
  kind: ShopProposalItem["kind"];
  unit: string;
  priceText: string;
  priceStated: boolean;
  vatText: string;
  vatBasis: string;
  note: string | null;
  status: "pending" | "created";
  error: string | null;
}

/** The delivery half of the proposal as the owner is editing it. Not
 *  `relevant` when nothing in the proposal ships. */
interface ShippingDraft {
  relevant: boolean;
  text: string;
  stated: boolean;
  status: "pending" | "saved";
  error: string | null;
}

/** A rate in basis points as percent text — `2100` reads "21", `2150`
 *  "21.50" — for the input the owner edits. */
function percentText(rateBp: number): string {
  return rateBp % 100 === 0 ? String(rateBp / 100) : priceInput(rateBp, 2);
}

function kindLabel(kind: ProposalRow["kind"]): string {
  if (kind === "stock") return strings.sitesShopSetupKindStock;
  if (kind === "dated") return strings.sitesShopSetupKindDated;
  return strings.sitesShopSetupKindService;
}

function rowsFrom(
  proposal: ShopConfigProposal,
  exponent: number,
): ProposalRow[] {
  return proposal.items.map((item, index) => ({
    key: index,
    included: true,
    name: item.name,
    kind: item.kind,
    unit: item.unit,
    priceText:
      item.price.state === "stated"
        ? priceInput(item.price.cents, exponent)
        : "",
    priceStated: item.price.state === "stated",
    vatText: percentText(item.vat_guess.rate_bp),
    vatBasis: item.vat_guess.basis,
    note: item.note,
    status: "pending",
    error: null,
  }));
}

function shippingFrom(
  proposal: ShopConfigProposal,
  exponent: number,
): ShippingDraft {
  const shipping = proposal.shipping;
  if (shipping.state === "not_needed") {
    return {
      relevant: false,
      text: "",
      stated: false,
      status: "pending",
      error: null,
    };
  }
  return {
    relevant: true,
    text:
      shipping.state === "stated" ? priceInput(shipping.cents, exponent) : "",
    stated: shipping.state === "stated",
    status: "pending",
    error: null,
  };
}

export function ShopSetupView() {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [products, setProducts] = useState<SiteTicketProductList | null>(null);
  const [currentShipping, setCurrentShipping] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [description, setDescription] = useState("");
  const [proposing, setProposing] = useState(false);
  const [proposeError, setProposeError] = useState<string | null>(null);
  const [unconfigured, setUnconfigured] = useState(false);
  const [rows, setRows] = useState<ProposalRow[] | null>(null);
  const [shipping, setShipping] = useState<ShippingDraft | null>(null);
  const [shippingNote, setShippingNote] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);

  const currency = products?.currency ?? "EUR";
  const exponent = products?.currencyExponent ?? 2;

  const load = useCallback(async () => {
    setLoading(true);
    try {
      // The detail first: the price list is a read the server refuses a
      // collaborator (S3.06a), and a screen that asks anyway greets them
      // with a refusal banner instead of the read-only fact (S3.06b).
      const detail = await api.site(siteId);
      const [items, shippingCents] = detail.canManageCollaborators
        ? await Promise.all([
            api.ticketProducts(siteId),
            api.shopShipping(siteId),
          ])
        : [null, null];
      setSite(detail);
      setProducts(items);
      setCurrentShipping(shippingCents);
      setError(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesShopSetupLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function propose() {
    setProposing(true);
    setProposeError(null);
    try {
      const proposal = await api.proposeShopConfig(description.trim());
      setRows(rowsFrom(proposal, exponent));
      setShipping(shippingFrom(proposal, exponent));
      setShippingNote(proposal.shipping_note);
    } catch (reason) {
      if (reason instanceof SitesError && reason.reason === "unconfigured") {
        setUnconfigured(true);
      } else {
        setProposeError(
          sitesMessage(reason, strings.sitesShopSetupProposeFailed),
        );
      }
    } finally {
      setProposing(false);
    }
  }

  function change(key: number, patch: Partial<ProposalRow>) {
    setRows((current) =>
      current === null
        ? current
        : current.map((row) => (row.key === key ? { ...row, ...patch } : row)),
    );
  }

  const pendingRows = (rows ?? []).filter(
    (row) => row.included && row.status === "pending",
  );
  const shippingPending =
    shipping !== null && shipping.relevant && shipping.status === "pending";

  /** Why Approve is disabled, in words — a disabled button that keeps its
   *  reason to itself is the S1.30b bug in a new place. `null` means go. */
  function requirement(): string | null {
    if (rows === null) return null;
    const anythingCreated = rows.some((row) => row.status === "created");
    if (pendingRows.length === 0 && !shippingPending) {
      return anythingCreated ? null : strings.sitesShopSetupNothingIncluded;
    }
    for (const row of pendingRows) {
      if (row.name.trim() === "") return strings.sitesShopSetupNameMissing;
      if (parsePriceInput(row.priceText, exponent) === null) {
        return strings.sitesShopSetupPriceMissing;
      }
      if (parsePriceInput(row.vatText, 2) === null) {
        return strings.sitesShopSetupVatMissing;
      }
    }
    if (
      shippingPending &&
      shipping !== null &&
      parsePriceInput(shipping.text, exponent) === null
    ) {
      return strings.sitesShopSetupShippingMissing;
    }
    return null;
  }

  async function approve() {
    if (rows === null) return;
    setApplying(true);
    let next = rows;
    for (const row of rows) {
      if (!row.included || row.status !== "pending") continue;
      const cents = parsePriceInput(row.priceText, exponent);
      const rateBp = parsePriceInput(row.vatText, 2);
      if (cents === null || rateBp === null) continue;
      try {
        await api.createShopProduct({
          name: row.name.trim(),
          unit: row.unit.trim(),
          unitPriceCents: cents,
          vatRateBp: rateBp,
          stocked: row.kind === "stock",
        });
        next = next.map((r) =>
          r.key === row.key
            ? { ...r, status: "created" as const, error: null }
            : r,
        );
      } catch (reason) {
        next = next.map((r) =>
          r.key === row.key
            ? {
                ...r,
                error: sitesMessage(reason, strings.sitesShopSetupCreateFailed),
              }
            : r,
        );
      }
      setRows(next);
    }
    if (
      shipping !== null &&
      shipping.relevant &&
      shipping.status === "pending"
    ) {
      const cents = parsePriceInput(shipping.text, exponent);
      if (cents !== null) {
        try {
          await api.setShopShipping(siteId, cents);
          setShipping({ ...shipping, status: "saved", error: null });
          setCurrentShipping(cents);
        } catch (reason) {
          setShipping({
            ...shipping,
            error: sitesMessage(reason, strings.sitesShopSetupShippingFailed),
          });
        }
      }
    }
    setApplying(false);
  }

  const manager = site !== null && site.canManageCollaborators;
  const created = (rows ?? []).filter((row) => row.status === "created");
  const failed = (rows ?? []).some((row) => row.error !== null);
  const allDone =
    rows !== null &&
    created.length > 0 &&
    pendingRows.length === 0 &&
    !shippingPending &&
    !failed &&
    (shipping === null || !shipping.relevant || shipping.status === "saved");
  const blocked = requirement();

  const manualPath = (
    <p className={styles.hint}>
      {strings.sitesShopSetupManualPath}{" "}
      <Link to="../tickets" relative="path">
        {strings.sitesShopSetupManualTickets}
      </Link>{" "}
      ·{" "}
      <Link to="../catalogs" relative="path">
        {strings.sitesShopSetupManualCatalogs}
      </Link>
    </p>
  );

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesShopSetup}</h1>
          {site !== null && (
            <span className={styles.submissionSiteName}>{site.name}</span>
          )}
        </div>
        <div className={styles.headerActions}>
          {loading && <Spinner size={16} />}
        </div>
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {loading && (
        <div
          className="flex min-h-72 items-center justify-center rounded-2xl border border-subtle bg-surface shadow-sm"
          role="status"
          aria-label={strings.sitesShopSetup}
        >
          <Spinner size={22} />
        </div>
      )}

      {/* A status, not a paragraph: the read-only fact arrives after the
          load, and a screen reader that has already moved past the header
          would otherwise never hear it (S3.06a, S3.06b). */}
      {!loading && site !== null && !manager && (
        <section
          className={styles.shopDescribe}
          aria-label={strings.sitesShopSetup}
        >
          <p className={styles.hint} role="status">
            {strings.sitesCommerceReadOnly}
          </p>
        </section>
      )}

      {!loading && manager && rows === null && (
        <section
          className={styles.shopDescribe}
          aria-label={strings.sitesShopSetup}
        >
          <p className={styles.shopIntro}>{strings.sitesShopSetupSubtitle}</p>
          {unconfigured ? (
            <p className={styles.hint} role="status">
              {strings.sitesShopSetupUnconfigured}
            </p>
          ) : (
            <>
              {proposeError !== null && <ErrorBanner message={proposeError} />}
              <Field
                label={strings.sitesShopSetupDescribeLabel}
                hint={strings.sitesShopSetupDescribeHint}
              >
                <textarea
                  className={`${styles.input} ${styles.textarea}`}
                  value={description}
                  maxLength={8000}
                  rows={5}
                  onChange={(event) => setDescription(event.target.value)}
                />
              </Field>
              <div className={styles.shopActions}>
                <Button
                  icon={<Sparkles size="var(--icon-size-inline)" />}
                  disabled={proposing || description.trim() === ""}
                  onClick={() => void propose()}
                >
                  {strings.sitesShopSetupPropose}
                </Button>
                {proposing && <Spinner size={16} />}
              </div>
            </>
          )}
          {products !== null && products.products.length > 0 && (
            <p className={styles.hint}>
              {strings.sitesShopSetupExisting(products.products.length)}
            </p>
          )}
          {manualPath}
        </section>
      )}

      {rows !== null && (
        <section
          className={styles.shopProposal}
          aria-label={strings.sitesShopSetupProposalTitle}
        >
          <h2 className={styles.sectionTitle}>
            {strings.sitesShopSetupProposalTitle}
          </h2>
          <p className={styles.shopIntro}>
            {strings.sitesShopSetupProposalIntro}
          </p>
          <ul className={styles.shopRows}>
            {rows.map((row) => (
              <li key={row.key} className={styles.shopRow}>
                <div className={styles.shopRowHead}>
                  <input
                    type="checkbox"
                    checked={row.included}
                    disabled={applying || row.status === "created"}
                    aria-label={strings.sitesShopSetupInclude(row.name)}
                    onChange={(event) =>
                      change(row.key, { included: event.target.checked })
                    }
                  />
                  <span className={styles.chip}>{kindLabel(row.kind)}</span>
                  {row.status === "created" && (
                    <span className={`${styles.chip} ${styles.chipLive}`}>
                      <Check
                        size="var(--icon-size-inline)"
                        aria-hidden="true"
                      />{" "}
                      {strings.sitesShopSetupCreated}
                    </span>
                  )}
                </div>
                <Field label={strings.sitesShopSetupItemName}>
                  <input
                    className={styles.input}
                    value={row.name}
                    disabled={
                      applying || !row.included || row.status === "created"
                    }
                    onChange={(event) =>
                      change(row.key, { name: event.target.value })
                    }
                  />
                </Field>
                <div className={styles.fieldRow}>
                  <Field
                    label={strings.sitesShopSetupItemPrice(currency)}
                    hint={
                      row.priceStated
                        ? undefined
                        : strings.sitesShopSetupPriceMissing
                    }
                  >
                    <input
                      className={styles.input}
                      inputMode="decimal"
                      value={row.priceText}
                      disabled={
                        applying || !row.included || row.status === "created"
                      }
                      onChange={(event) =>
                        change(row.key, { priceText: event.target.value })
                      }
                    />
                  </Field>
                  <Field label={strings.sitesShopSetupVatLabel}>
                    <input
                      className={styles.input}
                      inputMode="decimal"
                      value={row.vatText}
                      disabled={
                        applying || !row.included || row.status === "created"
                      }
                      onChange={(event) =>
                        change(row.key, { vatText: event.target.value })
                      }
                    />
                  </Field>
                </div>
                {row.unit !== "" && (
                  <Field label={strings.sitesShopSetupItemUnit}>
                    <input
                      className={styles.input}
                      value={row.unit}
                      disabled={
                        applying || !row.included || row.status === "created"
                      }
                      onChange={(event) =>
                        change(row.key, { unit: event.target.value })
                      }
                    />
                  </Field>
                )}
                <p className={styles.shopGuess}>
                  <span className={styles.badge}>
                    {strings.sitesShopSetupVatGuessBadge}
                  </span>{" "}
                  {row.vatBasis}
                </p>
                {row.note !== null && <p className={styles.hint}>{row.note}</p>}
                {row.error !== null && <ErrorBanner message={row.error} />}
              </li>
            ))}
          </ul>

          <div className={styles.shopShipping}>
            <h3 className={styles.shopShippingTitle}>
              {strings.sitesShopSetupShippingTitle}
            </h3>
            {shipping !== null && !shipping.relevant && (
              <p className={styles.hint}>
                {strings.sitesShopSetupShippingNotNeeded}
              </p>
            )}
            {shipping !== null && shipping.relevant && (
              <>
                <Field
                  label={strings.sitesShopSetupShippingLabel(currency)}
                  hint={
                    shipping.stated
                      ? undefined
                      : strings.sitesShopSetupShippingMissing
                  }
                >
                  <input
                    className={styles.input}
                    inputMode="decimal"
                    value={shipping.text}
                    disabled={applying || shipping.status === "saved"}
                    onChange={(event) =>
                      setShipping((current) =>
                        current === null
                          ? current
                          : { ...current, text: event.target.value },
                      )
                    }
                  />
                </Field>
                {currentShipping !== null && shipping.status === "pending" && (
                  <p className={styles.hint}>
                    {strings.sitesShopSetupShippingCurrent(
                      formatPrice(currentShipping, currency, exponent),
                    )}
                  </p>
                )}
                {shipping.status === "saved" && (
                  <p className={styles.hint} role="status">
                    {strings.sitesShopSetupShippingSaved}
                  </p>
                )}
                {shipping.error !== null && (
                  <ErrorBanner message={shipping.error} />
                )}
              </>
            )}
            {shippingNote !== null && (
              <p className={styles.hint}>{shippingNote}</p>
            )}
          </div>

          {allDone ? (
            <div className={styles.shopDone} role="status">
              <p>{strings.sitesShopSetupDone(created.length)}</p>
              {created.some((row) => row.kind === "dated") && (
                <Link to="../tickets" relative="path">
                  {strings.sitesShopSetupNextTickets}
                </Link>
              )}
            </div>
          ) : (
            <>
              <div className={styles.shopActions}>
                <Button
                  disabled={applying || blocked !== null}
                  onClick={() => void approve()}
                >
                  {failed
                    ? strings.sitesShopSetupRetry
                    : strings.sitesShopSetupApprove(pendingRows.length)}
                </Button>
                <Button
                  variant="ghost"
                  disabled={applying}
                  onClick={() => {
                    setRows(null);
                    setShipping(null);
                    setShippingNote(null);
                  }}
                >
                  {strings.sitesShopSetupDiscard}
                </Button>
                {applying && <Spinner size={16} />}
              </div>
              {blocked !== null && (
                <p className={styles.submitRequirement}>{blocked}</p>
              )}
            </>
          )}
        </section>
      )}
    </div>
  );
}
