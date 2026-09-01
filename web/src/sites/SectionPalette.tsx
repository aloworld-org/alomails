// The section palette (ADR 0042 §4, S3.01d): the blocks a page can be built
// from, shown as one scannable library without a competing preview pane.
//
// It is a focused popup library. A short category navigation keeps eighteen
// block types scannable, while the insertion control makes exact placement
// available without requiring a drag gesture through a covered page.
//
import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  BriefcaseBusiness,
  FileText,
  LayoutGrid,
  PanelsTopLeft,
  Settings2,
} from "lucide-react";

import { strings } from "../i18n";
import { ModuleNavigation, moduleNavigationItemClassName } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { kindDescription, kindLabel } from "./sectionInfo";
import { sectionThumbnail } from "./sectionThumbnails";
import { unseededPalette, type PaletteTile } from "./palette";
import type { Section, SectionKind } from "./sections";
import { ErrorBanner } from "./parts";
import styles from "./SitesModule.module.css";

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
  excludedKinds = [],
  initialPosition,
  onChoose,
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
  /** Website-level structure that must not be offered as a page block. */
  excludedKinds?: readonly SectionKind[];
  initialPosition?: number;
  /** A tile was chosen: insert its seeded section at `index`, or open the prop
   *  form there when it carries none. */
  onChoose: (tile: PaletteTile, index: number) => void;
}) {
  const api = useSitesApi();
  const [tiles, setTiles] = useState<PaletteTile[]>(unseededPalette());
  const [loading, setLoading] = useState(seeded);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [category, setCategory] = useState("all");
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
    panel.current
      ?.querySelector<HTMLButtonElement>("[data-palette-tile]")
      ?.focus();
  }, [loading]);

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

  function select(tile: PaletteTile) {
    setSelected(tile.kind);
  }

  const availableTiles = tiles.filter(
    (tile) => !excludedKinds.includes(tile.kind),
  );
  const shown = availableTiles.find((tile) => tile.kind === selected) ?? null;
  const categories: {
    id: string;
    label: string;
    kinds: readonly SectionKind[] | null;
    icon: ReactNode;
  }[] = [
    {
      id: "all",
      label: strings.sitesPaletteCategoryAll,
      kinds: null,
      icon: <LayoutGrid size="var(--icon-size-inline)" />,
    },
    {
      id: "essentials",
      label: strings.sitesPaletteCategoryEssentials,
      kinds: ["nav", "hero", "features", "cta", "footer"],
      icon: <PanelsTopLeft size="var(--icon-size-inline)" />,
    },
    {
      id: "content",
      label: strings.sitesPaletteCategoryContent,
      kinds: ["text_image", "gallery", "testimonials", "team", "faq"],
      icon: <FileText size="var(--icon-size-inline)" />,
    },
    {
      id: "business",
      label: strings.sitesPaletteCategoryBusiness,
      kinds: [
        "pricing",
        "contact_form",
        "collection",
        "catalog",
        "booking",
        "tickets",
        "shop",
      ],
      icon: <BriefcaseBusiness size="var(--icon-size-inline)" />,
    },
    {
      id: "advanced",
      label: strings.sitesPaletteCategoryAdvanced,
      kinds: ["custom_code"],
      icon: <Settings2 size="var(--icon-size-inline)" />,
    },
  ];
  const activeCategory =
    categories.find((item) => item.id === category) ?? categories[0]!;
  const visibleTiles =
    activeCategory.kinds === null
      ? availableTiles
      : availableTiles.filter((tile) =>
          activeCategory.kinds?.includes(tile.kind),
        );
  const hasNavigation = sections.some((section) => section.type === "nav");
  const position = initialPosition ?? sections.length;
  const positions = positionOptions(sections);
  const positionLabel =
    shown?.kind === "nav"
      ? strings.sitesNavPinned
      : (positions.find((option) => option.index === position)?.label ??
        strings.sitesPaletteAtEnd);

  return (
    <section
      ref={panel}
      className={styles.palette}
      aria-label={strings.sitesPaletteTitle}
    >
      <p className={styles.paletteHint}>{strings.sitesPaletteHint}</p>
      <div className={styles.paletteToolbar}>
        <ModuleNavigation label={strings.sitesPaletteCategories}>
          <div className="contents" role="tablist">
            {categories.map((item) => {
              const active = category === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  role="tab"
                  aria-selected={active}
                  className={moduleNavigationItemClassName(active)}
                  onClick={() => {
                    setCategory(item.id);
                    const first =
                      item.kinds === null
                        ? availableTiles[0]
                        : availableTiles.find((tile) =>
                            item.kinds?.includes(tile.kind),
                          );
                    if (first !== undefined) select(first);
                  }}
                >
                  {item.icon}
                  {item.label}
                </button>
              );
            })}
          </div>
        </ModuleNavigation>
      </div>

      {error !== null && <ErrorBanner message={error} />}

      <div className={`${styles.paletteBody} !grid-cols-1`}>
        <div className={styles.paletteGrid}>
          {visibleTiles.map((tile) => (
            <button
              key={tile.kind}
              type="button"
              data-palette-tile={tile.kind}
              className={
                selected === tile.kind
                  ? `${styles.paletteTile} ${styles.paletteTileActive}`
                  : styles.paletteTile
              }
              aria-label={
                tile.kind === "nav" && hasNavigation
                  ? `${kindLabel(tile.kind)} — ${strings.sitesNavAlreadyAdded}`
                  : strings.sitesPaletteAdd(
                      kindLabel(tile.kind),
                      tile.kind === "nav"
                        ? strings.sitesNavPinned
                        : positionLabel,
                    )
              }
              disabled={busy || (tile.kind === "nav" && hasNavigation)}
              onFocus={() => select(tile)}
              onMouseEnter={() => select(tile)}
              onClick={() => onChoose(tile, tile.kind === "nav" ? 0 : position)}
            >
              <svg
                className={styles.paletteThumb}
                viewBox="0 0 64 40"
                aria-hidden="true"
                focusable="false"
              >
                {sectionThumbnail(tile.kind)}
              </svg>
              <span className={styles.paletteTileName}>
                {kindLabel(tile.kind)}
              </span>
              <span className={styles.paletteTileDesc}>
                {kindDescription(tile.kind)}
              </span>
              {tile.kind === "nav" && hasNavigation ? (
                <span className={styles.paletteTileNeeds}>
                  {strings.sitesNavAlreadyAdded}
                </span>
              ) : tile.section === null && seeded ? (
                <span className={styles.paletteTileNeeds}>
                  {strings.sitesPaletteOpensForm}
                </span>
              ) : null}
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}
