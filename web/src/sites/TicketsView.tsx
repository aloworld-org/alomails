// The owner's box office (ADR 0041, S3.04f3): the dated events a site sells
// seats to, over the `/sites/{id}/tickets` routes.
//
// Three facts shape this screen, all inherited from the store rather than
// invented here:
//
//   * **Nothing here is a second copy.** An event names an item on Billing's
//     price list; the name and the price in every row are the list's answer
//     at this read, and an event whose item was archived says so instead of
//     showing a price that is no longer anyone's.
//   * **Capacity is the only edit.** Once an event exists its date and its
//     product are what people bought seats to; growing capacity is always
//     allowed, shrinking stops at the seats already spoken for — the server's
//     refusal sentence is what a person reads.
//   * **Sold seats are a record.** Delete works while nobody has bought in
//     and is refused afterwards, in the server's own words.
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, Plus, Ticket, Trash2 } from "lucide-react";
import { Link, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { formatPrice } from "./catalogPricing";
import { DialogFrame, EmptyState, ErrorBanner, Field } from "./parts";
import type {
  SiteDetail,
  SiteTicketEvent,
  SiteTicketEventList,
  SiteTicketProductList,
} from "./types";
import styles from "./SitesModule.module.css";

const when = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

/** The value a `datetime-local` input wants (`YYYY-MM-DDTHH:mm`), local. */
function toInputValue(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}

/** What the picker proposes for a new event: a week from now, in the
 *  evening — a real suggestion rather than a blank to decode. */
function suggestedStart(): string {
  const suggestion = new Date();
  suggestion.setDate(suggestion.getDate() + 7);
  suggestion.setHours(19, 0, 0, 0);
  return toInputValue(suggestion);
}

/** The instant a `datetime-local` value names, as RFC 3339 UTC — what the
 *  route's `startsAt` takes — or `null` while the field is not a moment. */
function toStartsAt(typed: string): string | null {
  if (typed === "") return null;
  const instant = new Date(typed);
  return Number.isNaN(instant.getTime()) ? null : instant.toISOString();
}

function NewEventDialog({
  products,
  busy,
  error,
  onClose,
  onCreate,
}: {
  products: SiteTicketProductList;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onCreate: (draft: { productId: string; startsAt: string; capacity: number }) => void;
}) {
  const [productId, setProductId] = useState(products.products[0]?.id ?? "");
  const [starts, setStarts] = useState(suggestedStart);
  const [capacity, setCapacity] = useState("50");
  const startsAt = toStartsAt(starts);
  const seats = Number.parseInt(capacity.trim(), 10);
  return (
    <DialogFrame
      Icon={Ticket}
      title={strings.sitesNewTicketEvent}
      subtitle={strings.sitesNewTicketEventSubtitle}
      error={error}
      busy={busy}
      canSubmit={productId !== "" && startsAt !== null && Number.isInteger(seats)}
      submitLabel={strings.sitesTicketCreateSubmit}
      onClose={onClose}
      onSubmit={() => {
        if (startsAt === null) return;
        onCreate({ productId, startsAt, capacity: Number.isInteger(seats) ? seats : 0 });
      }}
    >
      <Field
        label={strings.sitesTicketEventProduct}
        hint={strings.sitesTicketEventProductHint}
      >
        <select
          className={styles.input}
          value={productId}
          onChange={(event) => setProductId(event.target.value)}
          autoFocus
        >
          {products.products.map((product) => (
            <option key={product.id} value={product.id}>
              {strings.sitesTicketProductOption(
                product.name,
                formatPrice(
                  product.unitPriceCents,
                  products.currency,
                  products.currencyExponent,
                ),
              )}
            </option>
          ))}
        </select>
      </Field>
      <Field label={strings.sitesTicketEventStartsAt}>
        <input
          className={styles.input}
          type="datetime-local"
          value={starts}
          onChange={(event) => setStarts(event.target.value)}
        />
      </Field>
      <Field
        label={strings.sitesTicketEventCapacity}
        hint={strings.sitesTicketEventCapacityHint}
      >
        <input
          className={styles.input}
          type="number"
          min={1}
          value={capacity}
          onChange={(event) => setCapacity(event.target.value)}
        />
      </Field>
    </DialogFrame>
  );
}

function CapacityDialog({
  event,
  busy,
  error,
  onClose,
  onSave,
}: {
  event: SiteTicketEvent;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onSave: (capacity: number) => void;
}) {
  const [capacity, setCapacity] = useState(String(event.capacity));
  const seats = Number.parseInt(capacity.trim(), 10);
  return (
    <DialogFrame
      Icon={Ticket}
      title={strings.sitesTicketCapacityTitle}
      subtitle={strings.sitesTicketCapacitySubtitle(event.sold + event.held)}
      error={error}
      busy={busy}
      canSubmit={Number.isInteger(seats)}
      submitLabel={strings.sitesTicketCapacitySubmit}
      onClose={onClose}
      onSubmit={() => onSave(Number.isInteger(seats) ? seats : 0)}
    >
      <Field
        label={strings.sitesTicketEventCapacity}
        hint={strings.sitesTicketEventCapacityHint}
      >
        <input
          className={styles.input}
          type="number"
          min={1}
          value={capacity}
          onChange={(event) => setCapacity(event.target.value)}
          autoFocus
        />
      </Field>
    </DialogFrame>
  );
}

export function TicketsView() {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [list, setList] = useState<SiteTicketEventList | null>(null);
  const [products, setProducts] = useState<SiteTicketProductList | null>(null);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [resizing, setResizing] = useState<SiteTicketEvent | null>(null);
  const [dialogBusy, setDialogBusy] = useState(false);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [armedId, setArmedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      // The detail first: whether the caller manages this site decides
      // whether the picker — a read of the whole price list the server
      // refuses a collaborator (S3.06a) — is asked for at all.
      const detail = await api.site(siteId);
      const [events, items] = await Promise.all([
        api.ticketEvents(siteId),
        detail.canManageCollaborators
          ? api.ticketProducts(siteId)
          : Promise.resolve(null),
      ]);
      setSite(detail);
      setList(events);
      setProducts(items);
      setError(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesTicketsLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  const events = useMemo(() => list?.events ?? [], [list]);

  async function create(draft: { productId: string; startsAt: string; capacity: number }) {
    setDialogBusy(true);
    setDialogError(null);
    try {
      await api.createTicketEvent(siteId, draft);
      setCreating(false);
      await load();
    } catch (reason) {
      setDialogError(sitesMessage(reason, strings.sitesTicketCreateFailed));
    } finally {
      setDialogBusy(false);
    }
  }

  async function resize(event: SiteTicketEvent, capacity: number) {
    setDialogBusy(true);
    setDialogError(null);
    try {
      const stored = await api.setTicketCapacity(siteId, event.id, capacity);
      setList((current) =>
        current === null
          ? current
          : {
              ...current,
              events: current.events.map((row) => (row.id === stored.id ? stored : row)),
            },
      );
      setResizing(null);
    } catch (reason) {
      setDialogError(sitesMessage(reason, strings.sitesTicketCapacityFailed));
    } finally {
      setDialogBusy(false);
    }
  }

  async function remove(event: SiteTicketEvent) {
    if (armedId !== event.id) {
      setArmedId(event.id);
      return;
    }
    setBusyId(event.id);
    setError(null);
    try {
      await api.deleteTicketEvent(siteId, event.id);
      setList((current) =>
        current === null
          ? current
          : { ...current, events: current.events.filter((row) => row.id !== event.id) },
      );
      setArmedId(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesTicketDeleteFailed));
    } finally {
      setBusyId(null);
    }
  }

  const manager = site !== null && site.canManageCollaborators;
  const noProducts = products !== null && products.products.length === 0;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesTickets}</h1>
          {site !== null && <span className={styles.submissionSiteName}>{site.name}</span>}
        </div>
        <div className={styles.headerActions}>
          {loading && <Spinner size={16} />}
          {manager && (
            <Button
              size="sm"
              icon={<Plus size="var(--icon-size-inline)" />}
              disabled={products === null || noProducts}
              onClick={() => {
                setDialogError(null);
                setCreating(true);
              }}
            >
              {strings.sitesNewTicketEvent}
            </Button>
          )}
        </div>
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {!loading && site !== null && !manager && (
        <p className={styles.hint}>{strings.sitesCommerceReadOnly}</p>
      )}

      {!loading && noProducts && (
        <EmptyState
          Icon={Ticket}
          title={strings.sitesTicketNoProducts}
          body={strings.sitesTicketNoProductsHint}
        />
      )}

      {!loading && manager && !noProducts && events.length === 0 && (
        <EmptyState
          Icon={Ticket}
          title={strings.sitesNoTicketEventsTitle}
          body={strings.sitesNoTicketEventsBody}
          cta={strings.sitesNewTicketEvent}
          onCta={() => {
            setDialogError(null);
            setCreating(true);
          }}
        />
      )}

      {!loading && site !== null && !manager && events.length === 0 && (
        <EmptyState
          Icon={Ticket}
          title={strings.sitesNoTicketEventsTitle}
          body={strings.sitesCommerceReadOnly}
        />
      )}

      {events.length > 0 && list !== null && (
        <div className={styles.tableWrapStatic}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.sitesTicketWhen}</th>
                <th scope="col">{strings.sitesTicketWhat}</th>
                <th scope="col">{strings.sitesTicketPrice}</th>
                <th scope="col">{strings.sitesTicketSeats}</th>
                {manager && <th scope="col">{strings.sitesColActions}</th>}
              </tr>
            </thead>
            <tbody>
              {events.map((event) => (
                <tr key={event.id}>
                  <td>
                    <time dateTime={event.startsAt}>
                      {when.format(new Date(event.startsAt))}
                    </time>
                  </td>
                  <td>
                    {event.productName ?? (
                      <span className={styles.hint}>{strings.sitesTicketGoneProduct}</span>
                    )}
                  </td>
                  <td>
                    {event.unitPriceCents === null
                      ? "—"
                      : formatPrice(
                          event.unitPriceCents,
                          list.currency,
                          list.currencyExponent,
                        )}
                  </td>
                  <td>
                    {strings.sitesTicketSeatsCell(
                      event.sold,
                      event.remaining,
                      event.capacity,
                    )}
                    {event.held > 0 && (
                      <span className={styles.hint}>
                        {" "}
                        {strings.sitesTicketHeld(event.held)}
                      </span>
                    )}
                  </td>
                  {manager && (
                    <td>
                      <div className={styles.catalogItemActions}>
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={busyId === event.id}
                          onClick={() => {
                            setDialogError(null);
                            setResizing(event);
                          }}
                        >
                          {strings.sitesTicketChangeCapacity}
                        </Button>
                        <Button
                          variant={armedId === event.id ? "danger" : "ghost"}
                          size="sm"
                          icon={<Trash2 size="var(--icon-size-inline)" />}
                          disabled={busyId === event.id}
                          onClick={() => void remove(event)}
                        >
                          {armedId === event.id
                            ? strings.sitesTicketDeleteConfirm
                            : strings.sitesTicketDelete}
                        </Button>
                      </div>
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {armedId !== null && <p className={styles.hint}>{strings.sitesTicketDeleteHint}</p>}

      {creating && products !== null && (
        <NewEventDialog
          products={products}
          busy={dialogBusy}
          error={dialogError}
          onClose={() => setCreating(false)}
          onCreate={(draft) => void create(draft)}
        />
      )}

      {resizing !== null && (
        <CapacityDialog
          event={resizing}
          busy={dialogBusy}
          error={dialogError}
          onClose={() => setResizing(null)}
          onSave={(capacity) => void resize(resizing, capacity)}
        />
      )}
    </div>
  );
}
