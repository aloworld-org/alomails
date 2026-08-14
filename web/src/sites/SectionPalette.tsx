// The section palette (ADR 0042 §4, S3.01d): the blocks a page can be built
// from, each shown with THIS website's own content.
//
// It is a panel beside the stack rather than a dialog over it, because the
// gesture the ADR asks for is dragging a block onto the page — and you cannot
// drag out of a modal onto the thing it covers. The pointer gesture and the
// keyboard one produce the identical request: a tile carries the section the
// server seeded, and the position comes either from where it was dropped or
// from the "where it goes" control, so nothing about a block depends on being
// able to drag.
//
// What a tile shows is not an illustration. Selecting one (hover, or focus
// while tabbing) renders it through the same renderer that publishes the site,
// in the site's own theme, with the owner's own words in it. A block the
// palette cannot fill from the website — a quote nobody has given, a picture
// nobody has uploaded — says so in that space instead, and opens the prop form
// the way it always did. Nothing is ever invented to fill a preview.
import { useCallback, useEffect, useRef, useState } from "react";
import { LayoutGrid, X } from "lucide-react";

import { strings } from "../i18n";
import { Button, IconButton, Spinner } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { kindDescription, kindLabel } from "./sectionInfo";
import { sectionThumbnail } from "./sectionThumbnails";
import { unseededPalette, type PaletteNeed, type PaletteTile } from "./palette";
import type { Section } from "./sections";
import { ErrorBanner } from "./parts";
import styles from "./SitesModule.module.css";

/** The sentence a block that cannot be seeded shows in place of its picture. */
function needMessage(need: PaletteNeed | null): string {
  switch (need) {
    case "picture":
      return strings.sitesPaletteNeedsPicture;
    case "catalog":
      return strings.sitesPaletteNeedsCatalog;
    case "collection":
      return strings.sitesPaletteNeedsCollection;
    case "booking":
      return strings.sitesPaletteNeedsBooking;
    case "code":
      return strings.sitesPaletteNeedsCode;
    case "writing":
      return strings.sitesPaletteNeedsWriting;
    default:
      return strings.sitesPaletteOpensForm;
  }
}

/** Every place a block can be inserted, in the words of the stack it joins. */
export function positionOptions(
  sections: Section[],
): { index: number; label: string }[] {
  const options = [{ index: 0, label: strings.sitesPaletteAtTop }];
  for (let i = 0; i < sections.length; i += 1) {
    const section = sections[i];
    if (section === undefined) continue;
    options.push({
      index: i + 1,
      label:
        i === sections.length - 1
          ? strings.sitesPaletteAtEnd
          : strings.sitesPaletteAfter(kindLabel(section.type)),
    });
  }
  return options;
}

export function SectionPalette({
  siteId,
  pageId,
  seeded,
  sections,
  busy,
  onChoose,
  onDragTile,
  onDragEnd,
  onClose,
}: {
  siteId: string;
  pageId: string;
  /** Whether the server can seed tiles for what is being edited. A translation
   *  is edited as a whole page rather than through the section ops, so its
   *  palette is the plain list of blocks and every tile opens the form. */
  seeded: boolean;
  /** The stack as it is now — what the position control names. */
  sections: Section[];
  busy: boolean;
  /** A tile was chosen: insert its seeded section at `index`, or open the prop
   *  form there when it carries none. */
  onChoose: (tile: PaletteTile, index: number) => void;
  /** A tile is being dragged; the stack shows where it would land. */
  onDragTile: (tile: PaletteTile) => void;
  onDragEnd: () => void;
  onClose: () => void;
}) {
  const api = useSitesApi();
  const [tiles, setTiles] = useState<PaletteTile[]>(unseededPalette());
  const [loading, setLoading] = useState(seeded);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [previews, setPreviews] = useState<Record<string, string>>({});
  const [previewBusy, setPreviewBusy] = useState(false);
  const [position, setPosition] = useState(sections.length);
  const panel = useRef<HTMLElement | null>(null);

  // Opening the palette moves the caret into it: it is a disclosure opened by
  // a button, and a panel nobody's focus is in is a panel a keyboard cannot
  // reach without tabbing back through the whole page. It waits for the tiles
  // the server seeded — focusing a tile that is about to be replaced would put
  // the caret back on the document when it is.
  const focused = useRef(false);
  useEffect(() => {
    if (loading || focused.current) return;
    focused.current = true;
    panel.current?.querySelector<HTMLButtonElement>("[data-palette-tile]")?.focus();
  }, [loading]);

  // The position control names sections that can change under it (an undo, an
  // AI edit, another tile dropped). "At the end" stays the end.
  useEffect(() => {
    setPosition((current) => (current >= sections.length ? sections.length : current));
  }, [sections.length]);

  useEffect(() => {
    if (!seeded) return undefined;
    let live = true;
    setLoading(true);
    api.pagePalette(siteId, pageId).then(
      (loaded) => {
        if (!live) return;
        setTiles(loaded);
        setError(null);
        setLoading(false);
      },
      (err: unknown) => {
        if (!live) return;
        // The palette is an enrichment: losing it costs the pictures and the
        // seeding, never the ability to add a section.
        setTiles(unseededPalette());
        setError(sitesMessage(err, strings.sitesPaletteFailed));
        setLoading(false);
      },
    );
    return () => {
      live = false;
    };
  }, [api, siteId, pageId, seeded]);

  const select = useCallback(
    (tile: PaletteTile) => {
      setSelected(tile.kind);
      if (tile.section === null || previews[tile.kind] !== undefined) return;
      setPreviewBusy(true);
      api.palettePreview(siteId, pageId, tile.kind).then(
        (html) => {
          setPreviews((current) => ({ ...current, [tile.kind]: html }));
          setPreviewBusy(false);
        },
        () => {
          // A picture that will not load is a missing picture, not an error
          // worth a banner: the tile still adds its block.
          setPreviewBusy(false);
        },
      );
    },
    [api, pageId, previews, siteId],
  );

  const shown = tiles.find((tile) => tile.kind === selected) ?? null;
  const positions = positionOptions(sections);
  const positionLabel =
    positions.find((option) => option.index === position)?.label ??
    strings.sitesPaletteAtEnd;

  return (
    <section
      ref={panel}
      className={styles.palette}
      aria-label={strings.sitesPaletteTitle}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <div className={styles.paletteHead}>
        <span className={styles.paletteIcon} aria-hidden="true">
          <LayoutGrid size={17} />
        </span>
        <div className={styles.paletteHeadText}>
          <h3 className={styles.paletteTitle}>{strings.sitesPaletteTitle}</h3>
          <p className={styles.paletteHint}>{strings.sitesPaletteHint}</p>
        </div>
        <label className={styles.palettePosition}>
          <span>{strings.sitesPalettePosition}</span>
          <select
            className={styles.select}
            value={position}
            disabled={busy}
            onChange={(event) => setPosition(Number(event.target.value))}
          >
            {positions.map((option) => (
              <option key={option.index} value={option.index}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <IconButton
          size="sm"
          label={strings.close}
          icon={<X size={16} />}
          onClick={onClose}
        />
      </div>

      {error !== null && <ErrorBanner message={error} />}

      <div className={styles.paletteBody}>
        <div className={styles.paletteGrid}>
          {tiles.map((tile) => (
            <button
              key={tile.kind}
              type="button"
              data-palette-tile={tile.kind}
              className={
                selected === tile.kind
                  ? `${styles.paletteTile} ${styles.paletteTileActive}`
                  : styles.paletteTile
              }
              draggable={!busy}
              aria-label={strings.sitesPaletteAdd(kindLabel(tile.kind), positionLabel)}
              disabled={busy}
              onFocus={() => select(tile)}
              onMouseEnter={() => select(tile)}
              onDragStart={(event) => {
                // Firefox refuses to start a drag with nothing on the
                // transfer; the kind is what is already visible on the tile.
                event.dataTransfer?.setData("text/plain", tile.kind);
                onDragTile(tile);
              }}
              onDragEnd={onDragEnd}
              onClick={() => onChoose(tile, position)}
            >
              <svg
                className={styles.paletteThumb}
                viewBox="0 0 64 40"
                aria-hidden="true"
                focusable="false"
              >
                {sectionThumbnail(tile.kind)}
              </svg>
              <span className={styles.paletteTileName}>{kindLabel(tile.kind)}</span>
              <span className={styles.paletteTileDesc}>{kindDescription(tile.kind)}</span>
              {tile.section === null && seeded && (
                <span className={styles.paletteTileNeeds}>
                  {strings.sitesPaletteOpensForm}
                </span>
              )}
            </button>
          ))}
        </div>

        <div className={styles.palettePreview}>
          {loading ? (
            <p className={styles.paletteNote}>
              <Spinner size={14} /> {strings.sitesPaletteLoading}
            </p>
          ) : shown === null ? (
            <p className={styles.paletteNote}>{strings.sitesPaletteHint}</p>
          ) : shown.section !== null && previews[shown.kind] !== undefined ? (
            <>
              <div className={styles.palettePreviewViewport}>
                {/* Sandboxed and inert: the document may run its own menu
                    script, but it never reaches this origin, and no click in a
                    picture of a block should follow a link. */}
                <iframe
                  className={styles.palettePreviewFrame}
                  title={strings.sitesPalettePreviewTitle(kindLabel(shown.kind))}
                  sandbox="allow-scripts"
                  srcDoc={previews[shown.kind]}
                />
              </div>
              <p className={styles.paletteNote}>{strings.sitesPaletteOwnContent}</p>
            </>
          ) : previewBusy ? (
            <p className={styles.paletteNote}>
              <Spinner size={14} /> {strings.sitesPaletteLoading}
            </p>
          ) : (
            <p className={styles.paletteNote}>
              {seeded ? needMessage(shown.needs) : strings.sitesPaletteOpensForm}
            </p>
          )}
        </div>
      </div>

      <p className={styles.paletteFoot}>
        <Button variant="ghost" size="sm" onClick={onClose}>
          {strings.sitesPaletteDone}
        </Button>
      </p>
    </section>
  );
}
