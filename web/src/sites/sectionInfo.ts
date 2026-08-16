// What the module says ABOUT a section: its display name, the one-line
// description on its picker tile, and the summary line its card shows in the
// stack. Presentation only — the wire shapes live in `sections.ts`. Every
// string resolves through the live catalog at call time, so a locale switch
// re-renders correctly.
import { strings } from "../i18n";
import type { Section, SectionKind } from "./sections";

/** The section type's display name. */
export function kindLabel(kind: SectionKind): string {
  switch (kind) {
    case "nav":
      return strings.sitesSectionNav;
    case "hero":
      return strings.sitesSectionHero;
    case "features":
      return strings.sitesSectionFeatures;
    case "text_image":
      return strings.sitesSectionTextImage;
    case "gallery":
      return strings.sitesSectionGallery;
    case "testimonials":
      return strings.sitesSectionTestimonials;
    case "pricing":
      return strings.sitesSectionPricing;
    case "team":
      return strings.sitesSectionTeam;
    case "faq":
      return strings.sitesSectionFaq;
    case "cta":
      return strings.sitesSectionCta;
    case "contact_form":
      return strings.sitesSectionContactForm;
    case "collection":
      return strings.sitesSectionCollection;
    case "catalog":
      return strings.sitesSectionCatalog;
    case "booking":
      return strings.sitesSectionBooking;
    case "tickets":
      return strings.sitesSectionTickets;
    case "shop":
      return strings.sitesSectionShop;
    case "custom_code":
      return strings.sitesSectionCustomCode;
    case "footer":
      return strings.sitesSectionFooter;
  }
}

/** The one-line description on the picker tile (and the form's subtitle). */
export function kindDescription(kind: SectionKind): string {
  switch (kind) {
    case "nav":
      return strings.sitesSectionNavDesc;
    case "hero":
      return strings.sitesSectionHeroDesc;
    case "features":
      return strings.sitesSectionFeaturesDesc;
    case "text_image":
      return strings.sitesSectionTextImageDesc;
    case "gallery":
      return strings.sitesSectionGalleryDesc;
    case "testimonials":
      return strings.sitesSectionTestimonialsDesc;
    case "pricing":
      return strings.sitesSectionPricingDesc;
    case "team":
      return strings.sitesSectionTeamDesc;
    case "faq":
      return strings.sitesSectionFaqDesc;
    case "cta":
      return strings.sitesSectionCtaDesc;
    case "contact_form":
      return strings.sitesSectionContactFormDesc;
    case "collection":
      return strings.sitesSectionCollectionDesc;
    case "catalog":
      return strings.sitesSectionCatalogDesc;
    case "booking":
      return strings.sitesSectionBookingDesc;
    case "tickets":
      return strings.sitesSectionTicketsDesc;
    case "shop":
      return strings.sitesSectionShopDesc;
    case "custom_code":
      return strings.sitesSectionCustomCodeDesc;
    case "footer":
      return strings.sitesSectionFooterDesc;
  }
}

/** Shortens long-form copy to one card line. */
function clip(text: string): string {
  const trimmed = text.trim();
  return trimmed.length > 80 ? `${trimmed.slice(0, 79)}…` : trimmed;
}

/** The card's summary line: the section's own words where it has a heading,
 *  a count of its entries where it does not. May be empty (a bare contact
 *  form) — the card then shows the type name alone. */
export function sectionSummary(section: Section): string {
  switch (section.type) {
    case "nav":
      return strings.sitesCountLinks(section.links.length);
    case "hero":
      return clip(section.heading);
    case "features":
      return section.heading ?? strings.sitesCountEntries(section.items.length);
    case "text_image":
      return section.heading ?? clip(section.body);
    case "gallery":
      return section.heading ?? strings.sitesCountImages(section.images.length);
    case "testimonials":
      return section.heading ?? strings.sitesCountEntries(section.items.length);
    case "pricing":
      return section.heading ?? strings.sitesCountEntries(section.tiers.length);
    case "team":
      return section.heading ?? strings.sitesCountEntries(section.members.length);
    case "faq":
      return section.heading ?? strings.sitesCountEntries(section.items.length);
    case "cta":
      return clip(section.heading);
    case "contact_form":
      return section.heading ?? "";
    case "collection":
      return section.heading ?? strings.sitesSectionCollection;
    case "catalog":
      return (
        section.heading ??
        (section.category === undefined
          ? strings.sitesSectionCatalog
          : strings.sitesCatalogSectionOneGroup(section.category))
      );
    case "booking":
      return section.heading ?? strings.sitesSectionBooking;
    case "tickets":
      return (
        section.heading ??
        (section.body === undefined ? strings.sitesSectionTickets : clip(section.body))
      );
    case "shop":
      return (
        section.heading ??
        (section.body === undefined ? strings.sitesSectionShop : clip(section.body))
      );
    case "custom_code":
      // The frame's accessible name is what a visitor is told this block is,
      // so it is also the honest line for the card when there is no heading.
      return clip(section.heading ?? section.title);
    case "footer":
      return section.text ?? strings.sitesCountLinks(section.links.length);
  }
}

/** The name of a resize control (`split`, `columns`, `shape`), in the
 *  language of the person editing. A control the server declares and this
 *  build has no word for falls back to its key rather than to an empty
 *  label — an unnamed button is worse than an untranslated one. */
export function layoutControlLabel(key: string): string {
  switch (key) {
    case "split":
      return strings.sitesLayoutSplit;
    case "columns":
      return strings.sitesLayoutColumns;
    case "shape":
      return strings.sitesLayoutShape;
    default:
      return key;
  }
}

/** The name of one declared value of a resize control. Same fallback rule as
 *  [`layoutControlLabel`]. */
export function layoutValueLabel(key: string, value: string): string {
  switch (`${key}/${value}`) {
    case "split/wide_image":
      return strings.sitesLayoutSplitWideImage;
    case "split/half":
      return strings.sitesLayoutSplitHalf;
    case "split/wide_text":
      return strings.sitesLayoutSplitWideText;
    case "columns/two":
      return strings.sitesLayoutColumnsTwo;
    case "columns/three":
      return strings.sitesLayoutColumnsThree;
    case "columns/four":
      return strings.sitesLayoutColumnsFour;
    case "shape/natural":
      return strings.sitesLayoutShapeNatural;
    case "shape/wide":
      return strings.sitesLayoutShapeWide;
    case "shape/square":
      return strings.sitesLayoutShapeSquare;
    case "shape/tall":
      return strings.sitesLayoutShapeTall;
    default:
      return value;
  }
}
