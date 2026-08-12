// The form for one thing a website offers — a dish, a room, a service, a
// course. One form for adding and for changing, because they are the same
// decision seen twice, and because the server replaces the item whole either
// way: every field it shows is sent every time.
//
// It rules on nothing. The handle rules, the price grammar, the length caps
// and the availability words all live in the store, and a refusal comes back
// as a sentence naming the rule that was broken — which is what the form
// shows, verbatim.
import { useState } from "react";
import { Tag } from "lucide-react";

import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { priceInput } from "./catalogPricing";
import { DialogFrame, Field } from "./parts";
import type {
  SiteCatalog,
  SiteCatalogAvailability,
  SiteCatalogCategory,
  SiteCatalogItem,
  SiteCatalogItemDraft,
} from "./types";
import styles from "./SitesModule.module.css";

/** The draft an existing item starts from, or a blank one for a new item. */
function draftFrom(
  item: SiteCatalogItem | null,
  catalog: SiteCatalog,
): SiteCatalogItemDraft {
  if (item === null) {
    return {
      name: "",
      slug: "",
      categoryId: null,
      description: "",
      price: "",
      priceNote: "",
      availability: "available",
    };
  }
  return {
    name: item.name,
    slug: item.slug,
    categoryId: item.categoryId,
    description: item.description ?? "",
    price: priceInput(item.priceCents, catalog.currencyExponent),
    priceNote: item.priceNote ?? "",
    availability: item.availability,
  };
}

const AVAILABILITIES: readonly SiteCatalogAvailability[] = [
  "available",
  "sold_out",
  "hidden",
];

export function availabilityLabel(availability: SiteCatalogAvailability): string {
  switch (availability) {
    case "sold_out":
      return strings.sitesCatalogSoldOut;
    case "hidden":
      return strings.sitesCatalogHidden;
    default:
      return strings.sitesCatalogAvailable;
  }
}

export function CatalogItemDialog({
  siteId,
  catalog,
  categories,
  item,
  onClose,
  onSaved,
}: {
  siteId: string;
  catalog: SiteCatalog;
  categories: SiteCatalogCategory[];
  /** The item being changed, or `null` when one is being added. */
  item: SiteCatalogItem | null;
  onClose: () => void;
  onSaved: (item: SiteCatalogItem) => void;
}) {
  const api = useSitesApi();
  const [draft, setDraft] = useState<SiteCatalogItemDraft>(() => draftFrom(item, catalog));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function change(patch: Partial<SiteCatalogItemDraft>) {
    setDraft((current) => ({ ...current, ...patch }));
  }

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      const saved = item === null
        ? await api.createCatalogItem(siteId, catalog.id, draft)
        : await api.updateCatalogItem(siteId, catalog.id, item.id, draft);
      onSaved(saved);
    } catch (err) {
      setError(sitesMessage(err, strings.sitesCatalogItemSaveFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Tag}
      title={item === null ? strings.sitesCatalogNewItem : strings.sitesCatalogEditItem(item.name)}
      subtitle={strings.sitesCatalogItemSubtitle}
      error={error}
      busy={busy}
      canSubmit={draft.name.trim() !== ""}
      submitLabel={item === null ? strings.sitesCatalogAddItem : strings.sitesCatalogSaveItem}
      onClose={onClose}
      onSubmit={() => void submit()}
    >
      <Field label={strings.sitesCatalogItemName}>
        <input
          className={styles.input}
          value={draft.name}
          onChange={(event) => change({ name: event.target.value })}
          autoFocus
        />
      </Field>
      <Field label={strings.sitesCatalogItemHandle} hint={strings.sitesCatalogItemHandleHint}>
        <input
          className={styles.input}
          value={draft.slug}
          placeholder={strings.sitesCatalogItemHandlePlaceholder}
          onChange={(event) => change({ slug: event.target.value })}
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
        />
      </Field>
      <Field
        label={strings.sitesCatalogItemPrice(catalog.currency)}
        hint={strings.sitesCatalogItemPriceHint}
      >
        <input
          className={styles.input}
          value={draft.price}
          inputMode="decimal"
          onChange={(event) => change({ price: event.target.value })}
        />
      </Field>
      <Field label={strings.sitesCatalogItemPriceNote} hint={strings.sitesCatalogItemPriceNoteHint}>
        <input
          className={styles.input}
          value={draft.priceNote}
          onChange={(event) => change({ priceNote: event.target.value })}
        />
      </Field>
      {categories.length > 0 && (
        <Field label={strings.sitesCatalogItemGroup}>
          <select
            className={styles.input}
            value={draft.categoryId ?? ""}
            onChange={(event) =>
              change({ categoryId: event.target.value === "" ? null : event.target.value })
            }
          >
            <option value="">{strings.sitesCatalogItemNoGroup}</option>
            {categories.map((category) => (
              <option key={category.id} value={category.id}>
                {category.name}
              </option>
            ))}
          </select>
        </Field>
      )}
      <Field label={strings.sitesCatalogItemDescription}>
        <textarea
          className={styles.input}
          rows={3}
          value={draft.description}
          onChange={(event) => change({ description: event.target.value })}
        />
      </Field>
      <Field
        label={strings.sitesCatalogItemAvailability}
        hint={strings.sitesCatalogAvailabilityHint}
      >
        <select
          className={styles.input}
          value={draft.availability}
          onChange={(event) =>
            change({ availability: event.target.value as SiteCatalogAvailability })
          }
        >
          {AVAILABILITIES.map((availability) => (
            <option key={availability} value={availability}>
              {availabilityLabel(availability)}
            </option>
          ))}
        </select>
      </Field>
    </DialogFrame>
  );
}
