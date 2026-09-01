// The editable spelling of each section — what the prop forms hold while the
// user types — and the two pure conversions between it and the wire shape.
// While editing, every optional is a present-but-blank value (an empty
// string, a link with nothing in it yet); on save, blanks become absent keys
// again so the stored JSON stays minimal. Props the form does not offer yet
// (a feature's icon token, a contact form's form_id) ride through untouched —
// editing a section must never silently strip what another writer put there.
//
// No validation happens here: the server rules on every save and its 422
// sentence names the broken rule.
import type {
  FaqItem,
  FeatureItem,
  FeaturesLayout,
  HeroAlignment,
  GalleryLayout,
  HeroAnimationSpeed,
  HeroAppearance,
  HeroContentWidth,
  HeroHeight,
  HeroLayout,
  HeroMediaAnimation,
  HeroTextAnimation,
  PricingTier,
  Section,
  SectionImage,
  SectionKind,
  SectionLink,
  SectionPresentation,
  NavAppearance,
  TeamMember,
  Testimonial,
  TextImageLayout,
  TransitionDirection,
  TransitionEffect,
  TransitionSpeed,
  TransitionTrigger,
} from "./sections";

export interface NavDraft {
  type: "nav";
  links: SectionLink[];
  cta: SectionLink;
  appearance?: NavAppearance | undefined;
}

export interface HeroDraft {
  type: "hero";
  heading: string;
  subheading: string;
  image: SectionImage;
  video_url: string;
  primary_cta: SectionLink;
  secondary_cta: SectionLink;
  button_count: 0 | 1 | 2;
  appearance?: HeroAppearance | undefined;
  layout: HeroLayout;
  height: HeroHeight;
  alignment: HeroAlignment;
  content_width: HeroContentWidth;
  text_animation: HeroTextAnimation;
  media_animation: HeroMediaAnimation;
  animation_speed: HeroAnimationSpeed;
}

export interface FeatureItemDraft {
  title: string;
  body: string;
  /** Not offered by the form; preserved from the stored section. */
  icon?: string | undefined;
}

export interface PresentableDraft {
  presentation: SectionPresentation;
}

export interface FeaturesDraft extends PresentableDraft {
  type: "features";
  heading: string;
  intro: string;
  items: FeatureItemDraft[];
  /** The chosen column count, carried untouched: the prop form does not offer
   *  it (resizing happens on the page, ADR 0042) and a save must never be the
   *  thing that throws it away. */
  columns?: string | undefined;
  layout: FeaturesLayout;
}

export interface TextImageDraft extends PresentableDraft {
  type: "text_image";
  heading: string;
  body: string;
  image: SectionImage;
  image_side: "left" | "right";
  /** The chosen split, carried untouched (see [`FeaturesDraft`]). */
  split?: string | undefined;
  layout: TextImageLayout;
}

export interface GalleryDraft extends PresentableDraft {
  type: "gallery";
  heading: string;
  images: SectionImage[];
  /** The chosen column count, carried untouched. */
  columns?: string | undefined;
  layout: GalleryLayout;
}

export interface TestimonialDraft {
  quote: string;
  author: string;
  role: string;
}

export interface TestimonialsDraft extends PresentableDraft {
  type: "testimonials";
  heading: string;
  items: TestimonialDraft[];
}

export interface TierDraft {
  name: string;
  price: string;
  period: string;
  description: string;
  /** The included-feature bullets as multiline text, one per line. */
  featuresText: string;
  cta: SectionLink;
  highlighted: boolean;
}

export interface PricingDraft extends PresentableDraft {
  type: "pricing";
  heading: string;
  intro: string;
  tiers: TierDraft[];
}

export interface MemberDraft {
  name: string;
  role: string;
  photo: SectionImage;
  bio: string;
}

export interface TeamDraft extends PresentableDraft {
  type: "team";
  heading: string;
  members: MemberDraft[];
  /** The chosen column count, carried untouched. */
  columns?: string | undefined;
}

export interface FaqItemDraft {
  question: string;
  answer: string;
}

export interface FaqDraft extends PresentableDraft {
  type: "faq";
  heading: string;
  items: FaqItemDraft[];
}

export interface CtaDraft extends PresentableDraft {
  type: "cta";
  heading: string;
  body: string;
  button: SectionLink;
}

export interface ContactFormDraft extends PresentableDraft {
  type: "contact_form";
  heading: string;
  body: string;
  success_message: string;
  /** Not offered by the form until the forms slice; preserved as stored. */
  form_id?: string | undefined;
}

export interface CollectionDraft extends PresentableDraft {
  type: "collection";
  collection_id: string;
  heading: string;
}

export interface CatalogDraft extends PresentableDraft {
  type: "catalog";
  catalog_id: string;
  heading: string;
  /** A group's handle, or "" for every group. Cleared when the catalog
   *  changes — a handle only means anything inside its own catalog. */
  category: string;
}

/** Which of the site's booking services this section offers, and the heading
 *  above it. Nothing else: the length, the week and the questions belong to the
 *  service and are edited where the service is. */
export interface BookingDraft extends PresentableDraft {
  type: "booking";
  booking_id: string;
  heading: string;
}

/** The ticket shop's door on a page: the heading and the owner's sentence
 *  above the link. The events, their prices and their seats are managed on
 *  the Tickets screen and read live — nothing of them is edited here. */
export interface TicketsDraft extends PresentableDraft {
  type: "tickets";
  heading: string;
  body: string;
}

/** The stock shop's door on a page: the heading and the owner's sentence
 *  above the link. The shelf, its prices and its stock are managed on the
 *  Shop screen and read live — nothing of them is edited here. */
export interface ShopDraft extends PresentableDraft {
  type: "shop";
  heading: string;
  body: string;
}

export interface TransitionDraft {
  type: "transition";
  effect: TransitionEffect;
  direction: TransitionDirection;
  speed: TransitionSpeed;
  trigger: TransitionTrigger;
  animate_out: boolean;
}

/** A custom-code block while it is being written. The script is held even
 *  while the capability that runs it is switched off, so turning the switch
 *  back on does not cost the code that was typed — but a saved block never
 *  carries a script it is not allowed to run: `toSection` sends the two
 *  together or neither, which is the biconditional the server checks. */
export interface CustomCodeDraft {
  type: "custom_code";
  heading: string;
  title: string;
  html: string;
  css: string;
  js: string;
  scripts: boolean;
  inline_images: boolean;
  /** As typed, so the number field can be emptied mid-edit. A value that is
   *  not a number is sent as one the server refuses by its own rule, rather
   *  than being silently corrected here. */
  height: string;
}

export interface FooterDraft {
  type: "footer";
  text: string;
  links: SectionLink[];
}

/** One section as the form edits it. */
export type SectionDraft =
  | NavDraft
  | HeroDraft
  | FeaturesDraft
  | TextImageDraft
  | GalleryDraft
  | TestimonialsDraft
  | PricingDraft
  | TeamDraft
  | FaqDraft
  | CtaDraft
  | ContactFormDraft
  | CollectionDraft
  | CatalogDraft
  | BookingDraft
  | TicketsDraft
  | ShopDraft
  | TransitionDraft
  | CustomCodeDraft
  | FooterDraft;

/** The height a new block starts at: tall enough to show something, short
 *  enough that an empty one does not push the rest of the page off screen. A
 *  required field never starts blank. */
export const DEFAULT_CUSTOM_CODE_HEIGHT_PX = 320;

export const DEFAULT_SECTION_PRESENTATION: SectionPresentation = {
  layout: "clean",
  spacing: "standard",
  width: "balanced",
  alignment: "left",
  background: "background",
  text: "text",
  button: "accent_1",
  button_hover: "accent_2",
  entrance: "none",
  speed: "smooth",
};

export const blankLink = (): SectionLink => ({ label: "", href: "" });
export const blankImage = (): SectionImage => ({ blob_id: "", alt: "" });
export const blankFeature = (): FeatureItemDraft => ({ title: "", body: "" });
export const blankTestimonial = (): TestimonialDraft => ({
  quote: "",
  author: "",
  role: "",
});
export const blankTier = (): TierDraft => ({
  name: "",
  price: "",
  period: "",
  description: "",
  featuresText: "",
  cta: blankLink(),
  highlighted: false,
});
export const blankMember = (): MemberDraft => ({
  name: "",
  role: "",
  photo: blankImage(),
  bio: "",
});
export const blankFaqItem = (): FaqItemDraft => ({ question: "", answer: "" });

const draftLink = (link?: SectionLink): SectionLink => ({
  label: link?.label ?? "",
  href: link?.href ?? "",
});

/** The crop, focal point and decorative flag ride through the editor
 *  untouched. The image form now offers all three (S2.07c), but the rule that
 *  matters is the older one: a save must never be the thing that throws away
 *  how an image was framed, including by a form that never showed it. */
const draftImage = (image?: SectionImage): SectionImage => ({
  ...image,
  blob_id: image?.blob_id ?? "",
  alt: image?.alt ?? "",
});

/** Lists the server requires at least one entry in start with one blank row
 *  instead of an empty editor. */
function seeded<T>(items: T[], blank: () => T): T[] {
  return items.length === 0 ? [blank()] : items;
}

/** The form's starting state: a fresh draft of `kind`, filled from `initial`
 *  when editing an existing section (the caller guarantees the kinds match —
 *  a mismatch reads as a fresh draft). */
export function toDraft(kind: SectionKind, initial?: Section): SectionDraft {
  const from = initial?.type === kind ? initial : undefined;
  switch (kind) {
    case "nav": {
      const s = from as (Section & { type: "nav" }) | undefined;
      return {
        type: "nav",
        links: seeded(s?.links ?? [], blankLink),
        cta: draftLink(s?.cta),
        appearance: s?.appearance,
      };
    }
    case "hero": {
      const s = from as (Section & { type: "hero" }) | undefined;
      return {
        type: "hero",
        heading: s?.heading ?? "",
        subheading: s?.subheading ?? "",
        image: draftImage(s?.image),
        video_url: s?.video_url ?? "",
        primary_cta: draftLink(s?.primary_cta),
        secondary_cta: draftLink(s?.secondary_cta),
        button_count: s?.secondary_cta !== undefined ? 2 : s?.primary_cta !== undefined ? 1 : 0,
        appearance: s?.appearance,
        layout: s?.layout ?? "centered",
        height: s?.height ?? "standard",
        alignment: s?.alignment ?? "center",
        content_width: s?.content_width ?? "balanced",
        text_animation: s?.text_animation ?? "none",
        media_animation: s?.media_animation ?? "none",
        animation_speed: s?.animation_speed ?? "smooth",
      };
    }
    case "features": {
      const s = from as (Section & { type: "features" }) | undefined;
      return {
        type: "features",
        heading: s?.heading ?? "",
        intro: s?.intro ?? "",
        items: seeded(
          (s?.items ?? []).map((i: FeatureItem) => ({
            title: i.title,
            body: i.body,
            icon: i.icon,
          })),
          blankFeature,
        ),
        columns: s?.columns,
        layout: s?.layout ?? "grid",
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "text_image": {
      const s = from as (Section & { type: "text_image" }) | undefined;
      return {
        type: "text_image",
        heading: s?.heading ?? "",
        body: s?.body ?? "",
        image: draftImage(s?.image),
        image_side: s?.image_side ?? "left",
        split: s?.split,
        layout: s?.layout ?? "split",
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "gallery": {
      const s = from as (Section & { type: "gallery" }) | undefined;
      return {
        type: "gallery",
        heading: s?.heading ?? "",
        images: seeded(
          (s?.images ?? []).map((i: SectionImage) => draftImage(i)),
          blankImage,
        ),
        columns: s?.columns,
        layout: s?.layout ?? "grid",
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "testimonials": {
      const s = from as (Section & { type: "testimonials" }) | undefined;
      return {
        type: "testimonials",
        heading: s?.heading ?? "",
        items: seeded(
          (s?.items ?? []).map((i: Testimonial) => ({
            quote: i.quote,
            author: i.author,
            role: i.role ?? "",
          })),
          blankTestimonial,
        ),
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "pricing": {
      const s = from as (Section & { type: "pricing" }) | undefined;
      return {
        type: "pricing",
        heading: s?.heading ?? "",
        intro: s?.intro ?? "",
        tiers: seeded(
          (s?.tiers ?? []).map((t: PricingTier) => ({
            name: t.name,
            price: t.price,
            period: t.period ?? "",
            description: t.description ?? "",
            featuresText: t.features.join("\n"),
            cta: draftLink(t.cta),
            highlighted: t.highlighted,
          })),
          blankTier,
        ),
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "team": {
      const s = from as (Section & { type: "team" }) | undefined;
      return {
        type: "team",
        heading: s?.heading ?? "",
        members: seeded(
          (s?.members ?? []).map((m: TeamMember) => ({
            name: m.name,
            role: m.role ?? "",
            photo: draftImage(m.photo),
            bio: m.bio ?? "",
          })),
          blankMember,
        ),
        columns: s?.columns,
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "faq": {
      const s = from as (Section & { type: "faq" }) | undefined;
      return {
        type: "faq",
        heading: s?.heading ?? "",
        items: seeded(
          (s?.items ?? []).map((i: FaqItem) => ({
            question: i.question,
            answer: i.answer,
          })),
          blankFaqItem,
        ),
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "cta": {
      const s = from as (Section & { type: "cta" }) | undefined;
      return {
        type: "cta",
        heading: s?.heading ?? "",
        body: s?.body ?? "",
        button: draftLink(s?.button),
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "contact_form": {
      const s = from as (Section & { type: "contact_form" }) | undefined;
      return {
        type: "contact_form",
        heading: s?.heading ?? "",
        body: s?.body ?? "",
        success_message: s?.success_message ?? "",
        form_id: s?.form_id,
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "collection": {
      const s = from as (Section & { type: "collection" }) | undefined;
      return {
        type: "collection",
        collection_id: s?.collection_id ?? "",
        heading: s?.heading ?? "",
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "catalog": {
      const s = from as (Section & { type: "catalog" }) | undefined;
      return {
        type: "catalog",
        catalog_id: s?.catalog_id ?? "",
        heading: s?.heading ?? "",
        category: s?.category ?? "",
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "booking": {
      const s = from as (Section & { type: "booking" }) | undefined;
      return {
        type: "booking",
        booking_id: s?.booking_id ?? "",
        heading: s?.heading ?? "",
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "tickets": {
      const s = from as (Section & { type: "tickets" }) | undefined;
      return {
        type: "tickets",
        heading: s?.heading ?? "",
        body: s?.body ?? "",
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "shop": {
      const s = from as (Section & { type: "shop" }) | undefined;
      return {
        type: "shop",
        heading: s?.heading ?? "",
        body: s?.body ?? "",
        presentation: s?.presentation ?? DEFAULT_SECTION_PRESENTATION,
      };
    }
    case "transition": {
      const s = from as (Section & { type: "transition" }) | undefined;
      return {
        type: "transition",
        effect: s?.effect ?? "fade",
        direction: s?.direction ?? "up",
        speed: s?.speed ?? "smooth",
        trigger: s?.trigger ?? "balanced",
        animate_out: s?.animate_out ?? false,
      };
    }
    case "custom_code": {
      const s = from as (Section & { type: "custom_code" }) | undefined;
      return {
        type: "custom_code",
        heading: s?.heading ?? "",
        title: s?.title ?? "",
        html: s?.html ?? "",
        css: s?.css ?? "",
        js: s?.js ?? "",
        scripts: s?.capabilities?.scripts ?? false,
        inline_images: s?.capabilities?.inline_images ?? false,
        height: String(s?.height_px ?? DEFAULT_CUSTOM_CODE_HEIGHT_PX),
      };
    }
    case "footer": {
      const s = from as (Section & { type: "footer" }) | undefined;
      return {
        type: "footer",
        text: s?.text ?? "",
        links: seeded(s?.links ?? [], blankLink),
      };
    }
  }
}

/** A trimmed required string (the server rules on blankness). */
const req = (value: string): string => value.trim();

/** A trimmed optional string: blank becomes absent. */
function opt(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed === "" ? undefined : trimmed;
}

/** An optional link: untouched-blank becomes absent; a half-filled one is
 *  sent as typed so the server can name what is missing. */
function optLink(link: SectionLink): SectionLink | undefined {
  const label = link.label.trim();
  const href = link.href.trim();
  return label === "" && href === "" ? undefined : { label, href };
}

const reqLink = (link: SectionLink): SectionLink => ({
  label: link.label.trim(),
  href: link.href.trim(),
});

/** An optional image: no blob id means no image. */
function optImage(image: SectionImage): SectionImage | undefined {
  const blob_id = image.blob_id.trim();
  return blob_id === ""
    ? undefined
    : { ...image, blob_id, alt: image.alt.trim() };
}

const reqImage = (image: SectionImage): SectionImage => ({
  ...image,
  blob_id: image.blob_id.trim(),
  alt: image.alt.trim(),
});

/** The frame height as the wire carries it. Anything a height cannot be —
 *  blank, a word, a negative, past the wire's `u16` — is sent as 0, so the
 *  server answers by naming the allowed range instead of failing to parse the
 *  envelope at all. */
function heightPx(typed: string): number {
  const value = Number.parseInt(typed.trim(), 10);
  return Number.isInteger(value) && value >= 0 && value <= 65_535 ? value : 0;
}

/** Rows the user added but never touched are dropped, not sent as errors. */
function pruned<T>(items: T[], isBlank: (item: T) => boolean): T[] {
  return items.filter((item) => !isBlank(item));
}

const linkBlank = (l: SectionLink): boolean =>
  l.label.trim() === "" && l.href.trim() === "";
const imageBlank = (i: SectionImage): boolean =>
  i.blob_id.trim() === "" && i.alt.trim() === "";

/** The wire section a draft saves as — trimmed, blanks turned back into
 *  absent keys (`JSON.stringify` drops `undefined` props). */
export function toSection(draft: SectionDraft): Section {
  switch (draft.type) {
    case "nav":
      return {
        type: "nav",
        links: pruned(draft.links, linkBlank).map(reqLink),
        cta: optLink(draft.cta),
        appearance: draft.appearance,
      };
    case "hero":
      return {
        type: "hero",
        heading: req(draft.heading),
        subheading: opt(draft.subheading),
        image: optImage(draft.image),
        video_url: opt(draft.video_url),
        primary_cta: draft.button_count >= 1 ? optLink(draft.primary_cta) : undefined,
        secondary_cta: draft.button_count === 2 ? optLink(draft.secondary_cta) : undefined,
        appearance: draft.appearance,
        layout: draft.layout,
        height: draft.height,
        alignment: draft.alignment,
        content_width: draft.content_width,
        text_animation:
          draft.text_animation === "none" ? undefined : draft.text_animation,
        media_animation:
          draft.media_animation === "none" ? undefined : draft.media_animation,
        animation_speed:
          draft.text_animation === "none" && draft.media_animation === "none"
            ? undefined
            : draft.animation_speed,
      };
    case "features":
      return {
        type: "features",
        heading: opt(draft.heading),
        intro: opt(draft.intro),
        items: pruned(
          draft.items,
          (i) => i.title.trim() === "" && i.body.trim() === "",
        ).map((i) => ({
          title: req(i.title),
          body: req(i.body),
          icon: i.icon,
        })),
        columns: draft.columns,
        layout: draft.layout,
        presentation: draft.presentation,
      };
    case "text_image":
      return {
        type: "text_image",
        heading: opt(draft.heading),
        body: req(draft.body),
        image: reqImage(draft.image),
        image_side: draft.image_side,
        split: draft.split,
        layout: draft.layout,
        presentation: draft.presentation,
      };
    case "gallery":
      return {
        type: "gallery",
        heading: opt(draft.heading),
        images: pruned(draft.images, imageBlank).map(reqImage),
        columns: draft.columns,
        layout: draft.layout,
        presentation: draft.presentation,
      };
    case "testimonials":
      return {
        type: "testimonials",
        heading: opt(draft.heading),
        items: pruned(
          draft.items,
          (i) => i.quote.trim() === "" && i.author.trim() === "",
        ).map((i) => ({
          quote: req(i.quote),
          author: req(i.author),
          role: opt(i.role),
        })),
        presentation: draft.presentation,
      };
    case "pricing":
      return {
        type: "pricing",
        heading: opt(draft.heading),
        intro: opt(draft.intro),
        tiers: pruned(
          draft.tiers,
          (t) => t.name.trim() === "" && t.price.trim() === "",
        ).map((t) => ({
          name: req(t.name),
          price: req(t.price),
          period: opt(t.period),
          description: opt(t.description),
          features: t.featuresText
            .split("\n")
            .map((line) => line.trim())
            .filter((line) => line !== ""),
          cta: optLink(t.cta),
          highlighted: t.highlighted,
        })),
        presentation: draft.presentation,
      };
    case "team":
      return {
        type: "team",
        heading: opt(draft.heading),
        members: pruned(draft.members, (m) => m.name.trim() === "").map(
          (m) => ({
            name: req(m.name),
            role: opt(m.role),
            photo: optImage(m.photo),
            bio: opt(m.bio),
          }),
        ),
        columns: draft.columns,
        presentation: draft.presentation,
      };
    case "faq":
      return {
        type: "faq",
        heading: opt(draft.heading),
        items: pruned(
          draft.items,
          (i) => i.question.trim() === "" && i.answer.trim() === "",
        ).map((i) => ({ question: req(i.question), answer: req(i.answer) })),
        presentation: draft.presentation,
      };
    case "cta":
      return {
        type: "cta",
        heading: req(draft.heading),
        body: opt(draft.body),
        button: reqLink(draft.button),
        presentation: draft.presentation,
      };
    case "contact_form":
      return {
        type: "contact_form",
        heading: opt(draft.heading),
        body: opt(draft.body),
        form_id: draft.form_id,
        success_message: opt(draft.success_message),
        presentation: draft.presentation,
      };
    case "collection":
      return {
        type: "collection",
        collection_id: req(draft.collection_id),
        heading: opt(draft.heading),
        presentation: draft.presentation,
      };
    case "catalog":
      return {
        type: "catalog",
        catalog_id: req(draft.catalog_id),
        heading: opt(draft.heading),
        category: opt(draft.category),
        presentation: draft.presentation,
      };
    case "booking":
      return {
        type: "booking",
        booking_id: req(draft.booking_id),
        heading: opt(draft.heading),
        presentation: draft.presentation,
      };
    case "tickets":
      return {
        type: "tickets",
        heading: opt(draft.heading),
        body: opt(draft.body),
        presentation: draft.presentation,
      };
    case "shop":
      return {
        type: "shop",
        heading: opt(draft.heading),
        body: opt(draft.body),
        presentation: draft.presentation,
      };
    case "transition":
      return {
        type: "transition",
        effect: draft.effect,
        direction: draft.direction,
        speed: draft.speed,
        trigger: draft.trigger,
        animate_out: draft.animate_out,
      };
    case "custom_code": {
      // A script is stored only together with the capability that runs it:
      // switching scripts off saves the block without its script rather than
      // saving bytes the browser is forbidden to execute.
      const js = draft.scripts ? opt(draft.js) : undefined;
      return {
        type: "custom_code",
        heading: opt(draft.heading),
        title: req(draft.title),
        html: draft.html.trim(),
        css: opt(draft.css),
        js,
        capabilities: {
          scripts: draft.scripts,
          inline_images: draft.inline_images,
        },
        height_px: heightPx(draft.height),
      };
    }
    case "footer":
      return {
        type: "footer",
        text: opt(draft.text),
        links: pruned(draft.links, linkBlank).map(reqLink),
      };
  }
}
