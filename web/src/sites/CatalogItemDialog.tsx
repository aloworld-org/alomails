// The form for one thing a website offers — a dish, a room, a service, a
// course. One form for adding and for changing, because they are the same
// decision seen twice, and because the server replaces the item whole either
// way: every field it shows is sent every time.
//
// It rules on nothing. The handle rules, the price grammar, the length caps
// and the availability words all live in the store, and a refusal comes back
// as a sentence naming the rule that was broken — which is what the form
// shows, verbatim.
import { useRef, useState } from "react";
import { ImagePlus, Tag, Trash2 } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { Button, IconButton } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { priceInput } from "./catalogPricing";
import { useImageSource } from "./imageSource";
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
      imageBlobId: null,
      imageAlt: "",
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
    imageBlobId: item.imageBlobId,
    imageAlt: item.imageAlt ?? "",
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


/**
 * The item's photograph: pick one, see it, describe it, or remove it.
 *
 * The picture goes through Drive like every other image in Sites, so it stays
 * a file the owner can find again rather than a byte-string owned by one
 * dialog. With no photo the field is an empty state that says what happens
 * without one, because "no photo" is a perfectly good answer and the form
 * should not look broken for taking it.
 */
function PhotoField({
  siteId,
  blobId,
  alt,
  onChange,
}: {
  siteId: string;
  blobId: string | null;
  alt: string;
  onChange: (patch: { imageBlobId?: string | null; imageAlt?: string }) => void;
}) {
  const jmap = useJmapClient();
  const fileInput = useRef<HTMLInputElement>(null);
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const source = useImageSource(siteId, blobId ?? "");

  function upload(file: File) {
    setUploading(true);
    setUploadError(null);
    jmap.driveUploadBlob(null, null, file).then(
      ({ blobId: uploaded }) => {
        // A new picture is not the old one, and the old words described the
        // old one: replacing the photo clears what was said about it.
        onChange({ imageBlobId: uploaded, imageAlt: "" });
        setUploading(false);
      },
      () => {
        setUploadError(strings.sitesUploadFailed);
        setUploading(false);
      },
    );
  }

  return (
    <Field label={strings.sitesCatalogItemPhoto}>
      <div className={styles.itemPhoto}>
        {blobId === null ? (
          <div className={styles.itemPhotoEmpty}>
            <span>{strings.sitesCatalogItemPhotoNone}</span>
            <span className={styles.hint}>{strings.sitesCatalogItemPhotoNoneHint}</span>
          </div>
        ) : (
          source !== null && (
            <img
              className={styles.itemPhotoPreview}
              src={source}
              alt={alt.trim() === "" ? strings.sitesCatalogItemPhotoPreview : alt}
            />
          )
        )}
        <div className={styles.itemPhotoActions}>
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              // Allow re-picking the same file after a remove.
              event.target.value = "";
              if (file !== undefined) upload(file);
            }}
          />
          <Button
            variant="ghost"
            size="sm"
            icon={<ImagePlus size={14} />}
            disabled={uploading}
            onClick={() => fileInput.current?.click()}
          >
            {blobId === null
              ? strings.sitesCatalogItemPhotoAdd
              : strings.sitesCatalogItemPhotoReplace}
          </Button>
          {blobId !== null && (
            <IconButton
              size="sm"
              label={strings.sitesCatalogItemPhotoRemove}
              icon={<Trash2 size={14} />}
              disabled={uploading}
              // The description goes with the picture: the server refuses one
              // without the other, and keeping it would describe nothing.
              onClick={() => onChange({ imageBlobId: null, imageAlt: "" })}
            />
          )}
        </div>
      </div>
      {uploadError !== null && (
        <p className={styles.hint} role="alert">
          {uploadError}
        </p>
      )}
      {blobId !== null && (
        <div className={styles.itemPhotoAlt}>
          <Field
            label={strings.sitesCatalogItemPhotoAlt}
            hint={strings.sitesCatalogItemPhotoAltHint}
          >
            <input
              className={styles.input}
              value={alt}
              onChange={(event) => onChange({ imageAlt: event.target.value })}
            />
          </Field>
          {alt.trim() === "" && (
            <p className={styles.hint} role="status">
              {strings.sitesCatalogItemPhotoAltMissing}
            </p>
          )}
        </div>
      )}
    </Field>
  );
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
      <PhotoField
        siteId={siteId}
        blobId={draft.imageBlobId}
        alt={draft.imageAlt}
        onChange={change}
      />
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
