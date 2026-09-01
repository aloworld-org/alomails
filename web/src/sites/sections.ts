// The TypeScript mirror of the sections schema v1 — the closed vocabulary of
// seventeen section types a page is stacked from, exactly as the server's
// `site_model` speaks it on the wire (`type`-tagged, snake_case props,
// absent optionals as absent keys). This file changes only when the schema
// version does; it carries NO validation — the store rules on every write and
// its 422 sentence names the broken rule.

/** The schema version this build speaks; every envelope sent carries it. */
export const SECTIONS_SCHEMA_VERSION = 1;

/** A link: visible text plus its target (site path, #fragment, or
 *  http(s)/mailto/tel — the server rejects everything scriptable). */
export interface SectionLink {
  label: string;
  href: string;
}

export type ThemeColorRole =
  | "background"
  | "text"
  | "border"
  | "accent_1"
  | "accent_2"
  | "accent_3"
  | "accent_4"
  | "accent_5";

/** Optional navigation styling references the reusable site-theme palette. */
export interface NavAppearance {
  background: ThemeColorRole;
  text: ThemeColorRole;
  hover: ThemeColorRole;
}

/** The visible rectangle of a source image, in basis points (ten-thousandths)
 *  of its width and height from the top-left corner. Absent means the whole
 *  image. */
export interface ImageCrop {
  x_bp: number;
  y_bp: number;
  width_bp: number;
  height_bp: number;
}

/** The point of an image to keep in frame when a layout crops it further, in
 *  basis points of the source. Absent means the centre of the crop. */
export interface ImageFocalPoint {
  x_bp: number;
  y_bp: number;
}

/** An image reference: a tenant blob id, its alt text, and how it is
 *  presented. A blank `alt` with `decorative` set means "nothing to describe";
 *  a blank `alt` without it means the alt text is not written yet. */
export interface SectionImage {
  blob_id: string;
  alt: string;
  crop?: ImageCrop | undefined;
  focal?: ImageFocalPoint | undefined;
  decorative?: boolean | undefined;
  /** The frame the image is shown in — one of the shapes its section
   *  declares (`/sites/config`); absent means the picture's own proportions. */
  shape?: string | undefined;
}

/** Top navigation bar; the logo comes from the theme. */
export interface NavSection {
  type: "nav";
  links: SectionLink[];
  cta?: SectionLink | undefined;
  appearance?: NavAppearance | undefined;
}

/** The page's lead banner. */
export type HeroLayout =
  "centered" | "split_right" | "split_left" | "background" | "editorial";
export type HeroHeight = "compact" | "standard" | "tall";
export type HeroAlignment = "left" | "center" | "right";
export type HeroContentWidth = "narrow" | "balanced" | "wide";

export interface HeroSection {
  type: "hero";
  heading: string;
  subheading?: string | undefined;
  image?: SectionImage | undefined;
  primary_cta?: SectionLink | undefined;
  secondary_cta?: SectionLink | undefined;
  layout?: HeroLayout | undefined;
  height?: HeroHeight | undefined;
  alignment?: HeroAlignment | undefined;
  content_width?: HeroContentWidth | undefined;
}

/** One entry in a features grid. `icon` is a token the renderer may not ship
 *  yet — the editor preserves it but does not offer it. */
export interface FeatureItem {
  title: string;
  body: string;
  icon?: string | undefined;
}

/** A grid of product/service features; at least one item. */
export interface FeaturesSection {
  type: "features";
  heading?: string | undefined;
  intro?: string | undefined;
  items: FeatureItem[];
  /** Cards per row on a wide screen; absent is the fluid grid. */
  columns?: string | undefined;
}

/** A text block alongside an image. */
export interface TextImageSection {
  type: "text_image";
  heading?: string | undefined;
  body: string;
  image: SectionImage;
  image_side: "left" | "right";
  /** How the row is divided between image and text; absent is equal columns. */
  split?: string | undefined;
}

/** An image gallery; at least one image. */
export interface GallerySection {
  type: "gallery";
  heading?: string | undefined;
  images: SectionImage[];
  /** Images per row on a wide screen; absent is the fluid grid. */
  columns?: string | undefined;
}

/** One customer quote. */
export interface Testimonial {
  quote: string;
  author: string;
  role?: string | undefined;
}

/** A row of customer quotes; at least one. */
export interface TestimonialsSection {
  type: "testimonials";
  heading?: string | undefined;
  items: Testimonial[];
}

/** One pricing tier. `price` is a display string ("€9/mo") — never parsed,
 *  never computed on. */
export interface PricingTier {
  name: string;
  price: string;
  period?: string | undefined;
  description?: string | undefined;
  features: string[];
  cta?: SectionLink | undefined;
  highlighted: boolean;
}

/** A pricing table; at least one tier. */
export interface PricingSection {
  type: "pricing";
  heading?: string | undefined;
  intro?: string | undefined;
  tiers: PricingTier[];
}

/** One person on a team section. */
export interface TeamMember {
  name: string;
  role?: string | undefined;
  photo?: SectionImage | undefined;
  bio?: string | undefined;
}

/** The people behind the business; at least one member. */
export interface TeamSection {
  type: "team";
  heading?: string | undefined;
  members: TeamMember[];
  /** People per row on a wide screen; absent is the fluid grid. */
  columns?: string | undefined;
}

/** One question/answer pair. */
export interface FaqItem {
  question: string;
  answer: string;
}

/** A frequently-asked-questions list; at least one pair. */
export interface FaqSection {
  type: "faq";
  heading?: string | undefined;
  items: FaqItem[];
}

/** A standalone call-to-action banner. */
export interface CtaSection {
  type: "cta";
  heading: string;
  body?: string | undefined;
  button: SectionLink;
}

/** A contact form. `form_id` is wired by the forms slice; until then the
 *  editor preserves it untouched and the section renders without a working
 *  submit. */
export interface ContactFormSection {
  type: "contact_form";
  heading?: string | undefined;
  body?: string | undefined;
  form_id?: string | undefined;
  success_message?: string | undefined;
}

/** A live grid resolved from one connected alo Base collection. */
export interface CollectionSection {
  type: "collection";
  collection_id: string;
  heading?: string | undefined;
}

/** What the site offers — dishes, rooms, services, courses — frozen from the
 *  tenant's own catalog at publish time. `category` is a group's HANDLE, not
 *  its id, and it belongs to `catalog_id`: choosing a different catalog makes
 *  any handle from the previous one meaningless. Whether the published section
 *  carries an order form is a switch on the catalog, not on this section. */
export interface CatalogSection {
  type: "catalog";
  catalog_id: string;
  heading?: string | undefined;
  category?: string | undefined;
}

/** Something a visitor may book — a consultation, a viewing, a test drive.
 *  `booking_id` names one of the site's own booking services (S2.13a); the
 *  length, the opening hours and the questions asked are that service's and are
 *  frozen into the next publish, never copied here. */
export interface BookingSection {
  type: "booking";
  booking_id: string;
  heading?: string | undefined;
}

/** The door to the site's ticket shop. Presentation only — an optional
 *  heading and an optional line of the owner's own words above the link.
 *  What is on sale, its price and what is left are live state read from the
 *  price list on `/tix`, one navigation away; the events themselves are
 *  managed on the Tickets screen, never stored here. */
export interface TicketsSection {
  type: "tickets";
  heading?: string | undefined;
  body?: string | undefined;
}

/** The door to the site's stock shop — the tickets trade made again for
 *  goods on a shelf. Presentation only: an optional heading and an optional
 *  line of the owner's own words above the link. What is on sale, its price
 *  and what is on the shelf are live state read from Billing's price list and
 *  Inventory's ledger on `/shop`, one navigation away; the shelf itself is
 *  managed on the Shop screen, never stored here. */
export interface ShopSection {
  type: "shop";
  heading?: string | undefined;
  body?: string | undefined;
}

/** What the sandboxed frame around a custom-code block may do. Every field is
 *  default-deny, and the set is deliberately small: neither capability opens a
 *  network, and none of them ever will (`site_custom_code.rs`). Absent on the
 *  wire when nothing is granted. */
export interface CustomCodeCapabilities {
  /** The block's `js` runs. Declared exactly when there is a script to run —
   *  the server refuses a script without it AND it without a script. */
  scripts: boolean;
  /** `data:` images decode inside the frame. Never a URL; there is nothing to
   *  fetch from. */
  inline_images: boolean;
}

/** The tenant's own HTML, CSS and JavaScript, published inside a sandboxed
 *  frame. The three parts are stored apart — a `<script>` in the markup is a
 *  refusal, not a surprise — and the document around them (its doctype, its
 *  `Content-Security-Policy`, its `<style>` and `<script>`) is assembled by the
 *  renderer, never here. `height_px` is authored because a frame with an opaque
 *  origin cannot be measured from the page around it. */
export interface CustomCodeSection {
  type: "custom_code";
  /** Rendered by the PAGE, above the frame, in the site's own type. */
  heading?: string | undefined;
  /** The frame's accessible name. Required: a frame without one is announced
   *  as "frame" and nothing else. */
  title: string;
  html: string;
  css?: string | undefined;
  js?: string | undefined;
  capabilities?: CustomCodeCapabilities | undefined;
  height_px: number;
}

/** The page footer. */
export interface FooterSection {
  type: "footer";
  text?: string | undefined;
  links: SectionLink[];
}

/** One section of a page — the closed v1 vocabulary. */
export type Section =
  | NavSection
  | HeroSection
  | FeaturesSection
  | TextImageSection
  | GallerySection
  | TestimonialsSection
  | PricingSection
  | TeamSection
  | FaqSection
  | CtaSection
  | ContactFormSection
  | CollectionSection
  | CatalogSection
  | BookingSection
  | TicketsSection
  | ShopSection
  | CustomCodeSection
  | FooterSection;

/** A section's wire tag. */
export type SectionKind = Section["type"];

/** The eighteen kinds in their natural page order — the picker's order. */
export const SECTION_KINDS: readonly SectionKind[] = [
  "nav",
  "hero",
  "features",
  "text_image",
  "gallery",
  "testimonials",
  "pricing",
  "team",
  "faq",
  "cta",
  "contact_form",
  "collection",
  "catalog",
  "booking",
  "tickets",
  "shop",
  "custom_code",
  "footer",
];

/** The versioned value a page's sections travel in. */
export interface SectionsEnvelope {
  schema_version: number;
  sections: Section[];
}
