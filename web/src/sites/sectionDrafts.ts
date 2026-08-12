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
  PricingTier,
  Section,
  SectionImage,
  SectionKind,
  SectionLink,
  TeamMember,
  Testimonial,
} from "./sections";

export interface NavDraft {
  type: "nav";
  links: SectionLink[];
  cta: SectionLink;
}

export interface HeroDraft {
  type: "hero";
  heading: string;
  subheading: string;
  image: SectionImage;
  primary_cta: SectionLink;
  secondary_cta: SectionLink;
}

export interface FeatureItemDraft {
  title: string;
  body: string;
  /** Not offered by the form; preserved from the stored section. */
  icon?: string | undefined;
}

export interface FeaturesDraft {
  type: "features";
  heading: string;
  intro: string;
  items: FeatureItemDraft[];
}

export interface TextImageDraft {
  type: "text_image";
  heading: string;
  body: string;
  image: SectionImage;
  image_side: "left" | "right";
}

export interface GalleryDraft {
  type: "gallery";
  heading: string;
  images: SectionImage[];
}

export interface TestimonialDraft {
  quote: string;
  author: string;
  role: string;
}

export interface TestimonialsDraft {
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

export interface PricingDraft {
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

export interface TeamDraft {
  type: "team";
  heading: string;
  members: MemberDraft[];
}

export interface FaqItemDraft {
  question: string;
  answer: string;
}

export interface FaqDraft {
  type: "faq";
  heading: string;
  items: FaqItemDraft[];
}

export interface CtaDraft {
  type: "cta";
  heading: string;
  body: string;
  button: SectionLink;
}

export interface ContactFormDraft {
  type: "contact_form";
  heading: string;
  body: string;
  success_message: string;
  /** Not offered by the form until the forms slice; preserved as stored. */
  form_id?: string | undefined;
}

export interface CollectionDraft {
  type: "collection";
  collection_id: string;
  heading: string;
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
  | FooterDraft;

export const blankLink = (): SectionLink => ({ label: "", href: "" });
export const blankImage = (): SectionImage => ({ blob_id: "", alt: "" });
export const blankFeature = (): FeatureItemDraft => ({ title: "", body: "" });
export const blankTestimonial = (): TestimonialDraft => ({ quote: "", author: "", role: "" });
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
 *  untouched: no prop form offers them yet (S2.07c), and a save must never be
 *  the thing that throws away how an image was framed. */
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
      const s = from as Section & { type: "nav" } | undefined;
      return { type: "nav", links: seeded(s?.links ?? [], blankLink), cta: draftLink(s?.cta) };
    }
    case "hero": {
      const s = from as Section & { type: "hero" } | undefined;
      return {
        type: "hero",
        heading: s?.heading ?? "",
        subheading: s?.subheading ?? "",
        image: draftImage(s?.image),
        primary_cta: draftLink(s?.primary_cta),
        secondary_cta: draftLink(s?.secondary_cta),
      };
    }
    case "features": {
      const s = from as Section & { type: "features" } | undefined;
      return {
        type: "features",
        heading: s?.heading ?? "",
        intro: s?.intro ?? "",
        items: seeded(
          (s?.items ?? []).map((i: FeatureItem) => ({ title: i.title, body: i.body, icon: i.icon })),
          blankFeature,
        ),
      };
    }
    case "text_image": {
      const s = from as Section & { type: "text_image" } | undefined;
      return {
        type: "text_image",
        heading: s?.heading ?? "",
        body: s?.body ?? "",
        image: draftImage(s?.image),
        image_side: s?.image_side ?? "left",
      };
    }
    case "gallery": {
      const s = from as Section & { type: "gallery" } | undefined;
      return {
        type: "gallery",
        heading: s?.heading ?? "",
        images: seeded((s?.images ?? []).map((i: SectionImage) => draftImage(i)), blankImage),
      };
    }
    case "testimonials": {
      const s = from as Section & { type: "testimonials" } | undefined;
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
      };
    }
    case "pricing": {
      const s = from as Section & { type: "pricing" } | undefined;
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
      };
    }
    case "team": {
      const s = from as Section & { type: "team" } | undefined;
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
      };
    }
    case "faq": {
      const s = from as Section & { type: "faq" } | undefined;
      return {
        type: "faq",
        heading: s?.heading ?? "",
        items: seeded(
          (s?.items ?? []).map((i: FaqItem) => ({ question: i.question, answer: i.answer })),
          blankFaqItem,
        ),
      };
    }
    case "cta": {
      const s = from as Section & { type: "cta" } | undefined;
      return {
        type: "cta",
        heading: s?.heading ?? "",
        body: s?.body ?? "",
        button: draftLink(s?.button),
      };
    }
    case "contact_form": {
      const s = from as Section & { type: "contact_form" } | undefined;
      return {
        type: "contact_form",
        heading: s?.heading ?? "",
        body: s?.body ?? "",
        success_message: s?.success_message ?? "",
        form_id: s?.form_id,
      };
    }
    case "collection": {
      const s = from as Section & { type: "collection" } | undefined;
      return {
        type: "collection",
        collection_id: s?.collection_id ?? "",
        heading: s?.heading ?? "",
      };
    }
    case "footer": {
      const s = from as Section & { type: "footer" } | undefined;
      return { type: "footer", text: s?.text ?? "", links: seeded(s?.links ?? [], blankLink) };
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
  return blob_id === "" ? undefined : { ...image, blob_id, alt: image.alt.trim() };
}

const reqImage = (image: SectionImage): SectionImage => ({
  ...image,
  blob_id: image.blob_id.trim(),
  alt: image.alt.trim(),
});

/** Rows the user added but never touched are dropped, not sent as errors. */
function pruned<T>(items: T[], isBlank: (item: T) => boolean): T[] {
  return items.filter((item) => !isBlank(item));
}

const linkBlank = (l: SectionLink): boolean => l.label.trim() === "" && l.href.trim() === "";
const imageBlank = (i: SectionImage): boolean => i.blob_id.trim() === "" && i.alt.trim() === "";

/** The wire section a draft saves as — trimmed, blanks turned back into
 *  absent keys (`JSON.stringify` drops `undefined` props). */
export function toSection(draft: SectionDraft): Section {
  switch (draft.type) {
    case "nav":
      return {
        type: "nav",
        links: pruned(draft.links, linkBlank).map(reqLink),
        cta: optLink(draft.cta),
      };
    case "hero":
      return {
        type: "hero",
        heading: req(draft.heading),
        subheading: opt(draft.subheading),
        image: optImage(draft.image),
        primary_cta: optLink(draft.primary_cta),
        secondary_cta: optLink(draft.secondary_cta),
      };
    case "features":
      return {
        type: "features",
        heading: opt(draft.heading),
        intro: opt(draft.intro),
        items: pruned(
          draft.items,
          (i) => i.title.trim() === "" && i.body.trim() === "",
        ).map((i) => ({ title: req(i.title), body: req(i.body), icon: i.icon })),
      };
    case "text_image":
      return {
        type: "text_image",
        heading: opt(draft.heading),
        body: req(draft.body),
        image: reqImage(draft.image),
        image_side: draft.image_side,
      };
    case "gallery":
      return {
        type: "gallery",
        heading: opt(draft.heading),
        images: pruned(draft.images, imageBlank).map(reqImage),
      };
    case "testimonials":
      return {
        type: "testimonials",
        heading: opt(draft.heading),
        items: pruned(
          draft.items,
          (i) => i.quote.trim() === "" && i.author.trim() === "",
        ).map((i) => ({ quote: req(i.quote), author: req(i.author), role: opt(i.role) })),
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
      };
    case "team":
      return {
        type: "team",
        heading: opt(draft.heading),
        members: pruned(draft.members, (m) => m.name.trim() === "").map((m) => ({
          name: req(m.name),
          role: opt(m.role),
          photo: optImage(m.photo),
          bio: opt(m.bio),
        })),
      };
    case "faq":
      return {
        type: "faq",
        heading: opt(draft.heading),
        items: pruned(
          draft.items,
          (i) => i.question.trim() === "" && i.answer.trim() === "",
        ).map((i) => ({ question: req(i.question), answer: req(i.answer) })),
      };
    case "cta":
      return {
        type: "cta",
        heading: req(draft.heading),
        body: opt(draft.body),
        button: reqLink(draft.button),
      };
    case "contact_form":
      return {
        type: "contact_form",
        heading: opt(draft.heading),
        body: opt(draft.body),
        form_id: draft.form_id,
        success_message: opt(draft.success_message),
      };
    case "collection":
      return {
        type: "collection",
        collection_id: req(draft.collection_id),
        heading: opt(draft.heading),
      };
    case "footer":
      return {
        type: "footer",
        text: opt(draft.text),
        links: pruned(draft.links, linkBlank).map(reqLink),
      };
  }
}
