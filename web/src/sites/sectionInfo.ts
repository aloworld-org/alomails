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
    case "footer":
      return section.text ?? strings.sitesCountLinks(section.links.length);
  }
}
