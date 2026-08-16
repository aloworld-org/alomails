// The per-type prop forms — one small form per section kind, sharing the same
// few field primitives, in the module's dialog chrome. The form edits a
// draft (`sectionDrafts.ts`) and hands the wire section up on save; the
// SERVER rules on content (blank required text, bad hrefs, empty lists) and
// its 422 sentence is shown here verbatim, so there is exactly one copy of
// every rule.
import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { Blocks, Plus, Sparkles, Trash2 } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { strings } from "../i18n";
import { Button, IconButton, Spinner } from "../ds";
import { kindDescription, kindLabel } from "./sectionInfo";
import {
  blankFaqItem,
  blankFeature,
  blankImage,
  blankLink,
  blankMember,
  blankTestimonial,
  blankTier,
  toDraft,
  toSection,
} from "./sectionDrafts";
import type {
  BookingDraft,
  CatalogDraft,
  ContactFormDraft,
  CollectionDraft,
  CtaDraft,
  FaqDraft,
  FeaturesDraft,
  FooterDraft,
  GalleryDraft,
  HeroDraft,
  NavDraft,
  PricingDraft,
  SectionDraft,
  TeamDraft,
  TestimonialsDraft,
  TextImageDraft,
  ShopDraft,
  TicketsDraft,
} from "./sectionDrafts";
import type { Section, SectionKind, SectionLink } from "./sections";
import type { SiteCopyAction, SiteEditEnvelope } from "./types";
import type {
  SiteBooking,
  SiteCatalog,
  SiteCatalogCategory,
  SiteCollection,
} from "./types";
import { sitesMessage, useSitesApi } from "./api";
import { CopyContext, useCopyContext } from "./copyContext";
import type { CopyContextValue } from "./copyContext";
import { CustomCodeFields } from "./CustomCodeFields";
import { ImageFields } from "./ImageFields";
import { DialogFrame, Field } from "./parts";
import styles from "./SitesModule.module.css";

// ---- field primitives -------------------------------------------------------

function CopyTools({ pointer, value }: { pointer: string; value: string }) {
  const context = useCopyContext();
  const api = useSitesApi();
  const [open, setOpen] = useState(false);
  const [tone, setTone] = useState("");
  const [proposal, setProposal] = useState<SiteEditEnvelope | null>(null);
  const [after, setAfter] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (context === null || value.trim() === "") return null;

  async function propose(action: SiteCopyAction) {
    if (context === null) return;
    setBusy(true);
    setError(null);
    try {
      const prepared = await api.proposePageCopyEdit(context.siteId, context.pageId, {
        target: context.target,
        pointer,
        action,
        ...(action === "tone" ? { tone: tone.trim() } : {}),
      });
      const operation = prepared.proposal.operations[0];
      if (operation?.op !== "rewrite_copy") throw new Error(strings.sitesAiCopyFailed);
      setProposal(prepared.proposal);
      setAfter(operation.text);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesAiCopyFailed));
    } finally {
      setBusy(false);
    }
  }

  async function apply() {
    if (context === null || proposal === null) return;
    setBusy(true);
    setError(null);
    try {
      context.onApplied(await api.applyPageEdit(context.siteId, context.pageId, proposal));
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesAiApplyFailed));
      setBusy(false);
    }
  }

  return (
    <div className={styles.copyTools}>
      <Button
        variant="ghost"
        icon={<Sparkles size={16} />}
        aria-expanded={open}
        onClick={() => {
          setOpen((shown) => !shown);
          setProposal(null);
          setError(null);
        }}
      >
        {strings.sitesAiImproveCopy}
      </Button>
      {open && proposal === null && (
        <div className={styles.copyChoices} aria-label={strings.sitesAiCopyActions}>
          <Button variant="ghost" disabled={busy} onClick={() => void propose("rewrite")}>
            {strings.sitesAiRewrite}
          </Button>
          <Button variant="ghost" disabled={busy} onClick={() => void propose("shorter")}>
            {strings.sitesAiShorter}
          </Button>
          <Button variant="ghost" disabled={busy} onClick={() => void propose("longer")}>
            {strings.sitesAiLonger}
          </Button>
          <div className={styles.copyTone}>
            <input
              className={styles.input}
              value={tone}
              maxLength={60}
              aria-label={strings.sitesAiTone}
              placeholder={strings.sitesAiTonePlaceholder}
              onChange={(event) => setTone(event.target.value)}
            />
            <Button
              variant="ghost"
              disabled={busy || tone.trim() === ""}
              onClick={() => void propose("tone")}
            >
              {strings.sitesAiUseTone}
            </Button>
          </div>
        </div>
      )}
      {open && proposal !== null && (
        <div className={styles.copyProposal} aria-live="polite">
          <div>
            <span>{strings.sitesAiCopyBefore}</span>
            <p>{value}</p>
          </div>
          <div>
            <span>{strings.sitesAiCopyAfter}</span>
            <p>{after}</p>
          </div>
          <p className={styles.copyProposalHint}>{strings.sitesAiPreviewHint}</p>
          <div className={styles.copyProposalActions}>
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() => {
                setProposal(null);
                setAfter("");
              }}
            >
              {strings.sitesAiDiscard}
            </Button>
            <Button disabled={busy} onClick={() => void apply()}>
              {busy ? strings.sitesAiApplying : strings.sitesAiApprove}
            </Button>
          </div>
        </div>
      )}
      {error !== null && <p className={styles.aiEditError} role="alert">{error}</p>}
    </div>
  );
}

function TextField({
  label,
  value,
  onChange,
  hint,
  mono = false,
  autoFocus = false,
  copyPointer,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  hint?: string;
  mono?: boolean;
  autoFocus?: boolean;
  copyPointer?: string;
}) {
  return (
    <Field label={label} hint={hint}>
      <input
        className={mono ? `${styles.input} ${styles.mono}` : styles.input}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        autoFocus={autoFocus}
        {...(mono ? { autoCapitalize: "none", autoCorrect: "off", spellCheck: false } : {})}
      />
      {copyPointer !== undefined && <CopyTools pointer={copyPointer} value={value} />}
    </Field>
  );
}

function LongTextField({
  label,
  value,
  onChange,
  hint,
  copyPointer,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  hint?: string;
  copyPointer?: string;
}) {
  return (
    <Field label={label} hint={hint}>
      <textarea
        className={`${styles.input} ${styles.textarea}`}
        value={value}
        rows={4}
        onChange={(e) => onChange(e.target.value)}
      />
      {copyPointer !== undefined && <CopyTools pointer={copyPointer} value={value} />}
    </Field>
  );
}

function CheckField({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className={styles.toggle}>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      {label}
    </label>
  );
}

/** A link's two inputs. Both blank means "no link" for optional slots. */
function LinkFields({
  legend,
  value,
  onChange,
}: {
  legend?: string;
  value: SectionLink;
  onChange: (patch: Partial<SectionLink>) => void;
}) {
  return (
    <fieldset className={styles.subGroup}>
      {legend !== undefined && <legend className={styles.subLegend}>{legend}</legend>}
      <div className={styles.fieldRow}>
        <Field label={strings.sitesFieldLinkLabel}>
          <input
            className={styles.input}
            value={value.label}
            onChange={(e) => onChange({ label: e.target.value })}
          />
        </Field>
        <Field label={strings.sitesFieldLinkHref}>
          <input
            className={`${styles.input} ${styles.mono}`}
            value={value.href}
            onChange={(e) => onChange({ href: e.target.value })}
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
          />
        </Field>
      </div>
    </fieldset>
  );
}

/** The repeating-entries editor every list prop shares: numbered groups with
 *  a remove button each, and one add button at the end. Order is the order
 *  on the page; entries left blank are dropped on save, not sent as errors. */
function ItemsEditor<T extends object>({
  addLabel,
  items,
  onChange,
  blank,
  render,
}: {
  addLabel: string;
  items: T[];
  onChange: (items: T[]) => void;
  blank: () => T;
  render: (item: T, update: (patch: Partial<T>) => void, index: number) => ReactNode;
}) {
  const update = (index: number) => (patch: Partial<T>) => {
    onChange(items.map((item, i) => (i === index ? { ...item, ...patch } : item)));
  };
  return (
    <div className={styles.itemsEditor}>
      {items.map((item, i) => (
        // Entries have no identity of their own — the position is the key.
        <div key={i} className={styles.itemGroup}>
          <div className={styles.itemGroupHead}>
            <span className={styles.itemGroupName}>{strings.sitesItemN(i + 1)}</span>
            <IconButton
              size="sm"
              label={strings.sitesRemoveItem}
              icon={<Trash2 size={14} />}
              onClick={() => onChange(items.filter((_, j) => j !== i))}
            />
          </div>
          {render(item, update(i), i)}
        </div>
      ))}
      <Button
        variant="ghost"
        size="sm"
        icon={<Plus size={14} />}
        onClick={() => onChange([...items, blank()])}
      >
        {addLabel}
      </Button>
    </div>
  );
}

// ---- the per-type field bodies ----------------------------------------------

type Change = (draft: SectionDraft) => void;

function NavFields({ draft, onChange }: { draft: NavDraft; onChange: Change }) {
  return (
    <>
      <ItemsEditor
        addLabel={strings.sitesAddLink}
        items={draft.links}
        onChange={(links) => onChange({ ...draft, links })}
        blank={blankLink}
        render={(link, update) => <LinkFields value={link} onChange={update} />}
      />
      <LinkFields
        legend={strings.sitesFieldButton}
        value={draft.cta}
        onChange={(patch) => onChange({ ...draft, cta: { ...draft.cta, ...patch } })}
      />
    </>
  );
}

function HeroFields({ draft, onChange }: { draft: HeroDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <TextField
        label={strings.sitesFieldSubheading}
        value={draft.subheading}
        onChange={(subheading) => onChange({ ...draft, subheading })}
        copyPointer="/subheading"
      />
      <ImageFields
        legend={strings.sitesFieldImage}
        value={draft.image}
        pointer="/image"
        onChange={(patch) => onChange({ ...draft, image: { ...draft.image, ...patch } })}
      />
      <LinkFields
        legend={strings.sitesFieldPrimaryButton}
        value={draft.primary_cta}
        onChange={(patch) => onChange({ ...draft, primary_cta: { ...draft.primary_cta, ...patch } })}
      />
      <LinkFields
        legend={strings.sitesFieldSecondaryButton}
        value={draft.secondary_cta}
        onChange={(patch) =>
          onChange({ ...draft, secondary_cta: { ...draft.secondary_cta, ...patch } })
        }
      />
    </>
  );
}

function FeaturesFields({ draft, onChange }: { draft: FeaturesDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesFieldIntro}
        value={draft.intro}
        onChange={(intro) => onChange({ ...draft, intro })}
        copyPointer="/intro"
      />
      <ItemsEditor
        addLabel={strings.sitesAddEntry}
        items={draft.items}
        onChange={(items) => onChange({ ...draft, items })}
        blank={blankFeature}
        render={(item, update, index) => (
          <>
            <TextField
              label={strings.sitesFieldItemTitle}
              value={item.title}
              onChange={(title) => update({ title })}
              copyPointer={`/items/${index}/title`}
            />
            <LongTextField
              label={strings.sitesFieldBody}
              value={item.body}
              onChange={(body) => update({ body })}
              copyPointer={`/items/${index}/body`}
            />
          </>
        )}
      />
    </>
  );
}

function TextImageFields({ draft, onChange }: { draft: TextImageDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesFieldBody}
        value={draft.body}
        onChange={(body) => onChange({ ...draft, body })}
        copyPointer="/body"
      />
      <ImageFields
        legend={strings.sitesFieldImage}
        value={draft.image}
        pointer="/image"
        onChange={(patch) => onChange({ ...draft, image: { ...draft.image, ...patch } })}
      />
      <Field label={strings.sitesFieldImageSide}>
        <select
          className={styles.input}
          value={draft.image_side}
          onChange={(e) =>
            onChange({ ...draft, image_side: e.target.value === "right" ? "right" : "left" })
          }
        >
          <option value="left">{strings.sitesSideLeft}</option>
          <option value="right">{strings.sitesSideRight}</option>
        </select>
      </Field>
    </>
  );
}

function GalleryFields({ draft, onChange }: { draft: GalleryDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <ItemsEditor
        addLabel={strings.sitesAddImage}
        items={draft.images}
        onChange={(images) => onChange({ ...draft, images })}
        blank={blankImage}
        render={(image, update, index) => (
          <ImageFields value={image} onChange={update} pointer={`/images/${index}`} />
        )}
      />
    </>
  );
}

function TestimonialsFields({ draft, onChange }: { draft: TestimonialsDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <ItemsEditor
        addLabel={strings.sitesAddEntry}
        items={draft.items}
        onChange={(items) => onChange({ ...draft, items })}
        blank={blankTestimonial}
        render={(item, update, index) => (
          <>
            <LongTextField
              label={strings.sitesFieldQuote}
              value={item.quote}
              onChange={(quote) => update({ quote })}
              copyPointer={`/items/${index}/quote`}
            />
            <TextField
              label={strings.sitesFieldAuthor}
              value={item.author}
              onChange={(author) => update({ author })}
            />
            <TextField
              label={strings.sitesFieldRole}
              value={item.role}
              onChange={(role) => update({ role })}
              copyPointer={`/items/${index}/role`}
            />
          </>
        )}
      />
    </>
  );
}

function PricingFields({ draft, onChange }: { draft: PricingDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesFieldIntro}
        value={draft.intro}
        onChange={(intro) => onChange({ ...draft, intro })}
        copyPointer="/intro"
      />
      <ItemsEditor
        addLabel={strings.sitesAddTier}
        items={draft.tiers}
        onChange={(tiers) => onChange({ ...draft, tiers })}
        blank={blankTier}
        render={(tier, update, index) => (
          <>
            <div className={styles.fieldRow}>
              <TextField
                label={strings.sitesFieldTierName}
                value={tier.name}
                onChange={(name) => update({ name })}
              />
              <TextField
                label={strings.sitesFieldPrice}
                value={tier.price}
                onChange={(price) => update({ price })}
              />
            </div>
            <TextField
              label={strings.sitesFieldPeriod}
              value={tier.period}
              onChange={(period) => update({ period })}
            />
            <TextField
              label={strings.sitesFieldTierDescription}
              value={tier.description}
              onChange={(description) => update({ description })}
              copyPointer={`/tiers/${index}/description`}
            />
            <LongTextField
              label={strings.sitesFieldTierFeatures}
              value={tier.featuresText}
              onChange={(featuresText) => update({ featuresText })}
              hint={strings.sitesTierFeaturesHint}
            />
            <LinkFields
              legend={strings.sitesFieldButton}
              value={tier.cta}
              onChange={(patch) => update({ cta: { ...tier.cta, ...patch } })}
            />
            <CheckField
              label={strings.sitesFieldHighlighted}
              checked={tier.highlighted}
              onChange={(highlighted) => update({ highlighted })}
            />
          </>
        )}
      />
    </>
  );
}

function TeamFields({ draft, onChange }: { draft: TeamDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <ItemsEditor
        addLabel={strings.sitesAddMember}
        items={draft.members}
        onChange={(members) => onChange({ ...draft, members })}
        blank={blankMember}
        render={(member, update, index) => (
          <>
            <div className={styles.fieldRow}>
              <TextField
                label={strings.sitesFieldMemberName}
                value={member.name}
                onChange={(name) => update({ name })}
              />
              <TextField
                label={strings.sitesFieldRole}
                value={member.role}
                onChange={(role) => update({ role })}
              />
            </div>
            <ImageFields
              legend={strings.sitesFieldPhoto}
              value={member.photo}
              pointer={`/members/${index}/photo`}
              onChange={(patch) => update({ photo: { ...member.photo, ...patch } })}
            />
            <LongTextField
              label={strings.sitesFieldBio}
              value={member.bio}
              onChange={(bio) => update({ bio })}
              copyPointer={`/members/${index}/bio`}
            />
          </>
        )}
      />
    </>
  );
}

function FaqFields({ draft, onChange }: { draft: FaqDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <ItemsEditor
        addLabel={strings.sitesAddQuestion}
        items={draft.items}
        onChange={(items) => onChange({ ...draft, items })}
        blank={blankFaqItem}
        render={(item, update, index) => (
          <>
            <TextField
              label={strings.sitesFieldQuestion}
              value={item.question}
              onChange={(question) => update({ question })}
              copyPointer={`/items/${index}/question`}
            />
            <LongTextField
              label={strings.sitesFieldAnswer}
              value={item.answer}
              onChange={(answer) => update({ answer })}
              copyPointer={`/items/${index}/answer`}
            />
          </>
        )}
      />
    </>
  );
}

function CtaFields({ draft, onChange }: { draft: CtaDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesFieldBody}
        value={draft.body}
        onChange={(body) => onChange({ ...draft, body })}
        copyPointer="/body"
      />
      <LinkFields
        legend={strings.sitesFieldButton}
        value={draft.button}
        onChange={(patch) => onChange({ ...draft, button: { ...draft.button, ...patch } })}
      />
    </>
  );
}

function ContactFormFields({ draft, onChange }: { draft: ContactFormDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesFieldBody}
        value={draft.body}
        onChange={(body) => onChange({ ...draft, body })}
        copyPointer="/body"
      />
      <TextField
        label={strings.sitesFieldSuccessMessage}
        value={draft.success_message}
        onChange={(success_message) => onChange({ ...draft, success_message })}
        copyPointer="/success_message"
      />
      <p className={styles.hint}>{strings.sitesContactFormHint}</p>
    </>
  );
}

function CollectionFields({ draft, onChange }: { draft: CollectionDraft; onChange: Change }) {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [collections, setCollections] = useState<SiteCollection[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void api.collections(siteId).then(
      (connected) => {
        if (cancelled) return;
        setCollections(connected);
        setError(null);
      },
      (reason: unknown) => {
        if (!cancelled) setError(sitesMessage(reason, strings.sitesCollectionsLoadFailed));
      },
    ).finally(() => {
      if (!cancelled) setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [api, siteId]);

  const firstCollectionId = collections[0]?.id;
  useEffect(() => {
    if (draft.collection_id === "" && firstCollectionId !== undefined) {
      onChange({ ...draft, collection_id: firstCollectionId });
    }
  }, [draft, firstCollectionId, onChange]);

  return (
    <>
      <TextField
        label={strings.sitesCollectionSectionHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      {loading ? (
        <div className={styles.collectionFieldLoading} role="status">
          <Spinner size={16} />
          <span>{strings.sitesCollectionsLoading}</span>
        </div>
      ) : collections.length === 0 ? (
        <div className={styles.collectionFieldEmpty}>
          <strong>{strings.sitesCollectionSectionNoConnections}</strong>
          <span>{strings.sitesCollectionSectionNoConnectionsHint}</span>
          <Button variant="ghost" onClick={() => navigate(`/sites/${siteId}/collections`)}>
            {strings.sitesConnectTable}
          </Button>
        </div>
      ) : (
        <label className={styles.field}>
          <span>{strings.sitesCollectionSectionChoose}</span>
          <select
            className={styles.input}
            value={draft.collection_id}
            onChange={(event) => onChange({ ...draft, collection_id: event.target.value })}
          >
            {collections.map((collection) => (
              <option key={collection.id} value={collection.id}>{collection.name}</option>
            ))}
          </select>
        </label>
      )}
      {error !== null && <p className={styles.aiEditError} role="alert">{error}</p>}
    </>
  );
}

/** Mapping a page to what the site sells: which catalog, and optionally one of
 *  its groups. The prices, names and pictures are not here — they are the
 *  catalog's, frozen into the next publish — so this form asks two questions
 *  and says the two things that surprise people: an edit shows up at the next
 *  publish, and taking orders is a switch on the catalog, not on this page. */
function CatalogFields({ draft, onChange }: { draft: CatalogDraft; onChange: Change }) {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [catalogs, setCatalogs] = useState<SiteCatalog[]>([]);
  const [groups, setGroups] = useState<SiteCatalogCategory[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void api
      .catalogs(siteId)
      .then(
        (stored) => {
          if (cancelled) return;
          setCatalogs(stored);
          setError(null);
        },
        (reason: unknown) => {
          if (!cancelled) setError(sitesMessage(reason, strings.sitesCatalogsLoadFailed));
        },
      )
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, siteId]);

  const firstCatalogId = catalogs[0]?.id;
  useEffect(() => {
    if (draft.catalog_id === "" && firstCatalogId !== undefined) {
      onChange({ ...draft, catalog_id: firstCatalogId });
    }
  }, [draft, firstCatalogId, onChange]);

  // The groups on offer are the chosen catalog's own. A group is named by its
  // handle in the section, so the list has to come from the server rather than
  // be typed — and a stored handle whose group has since been deleted stays
  // selectable, because silently widening a section to the whole catalog would
  // publish something nobody asked for.
  const chosenId = draft.catalog_id;
  useEffect(() => {
    if (chosenId === "") {
      setGroups([]);
      return;
    }
    let cancelled = false;
    void api.catalog(siteId, chosenId).then(
      (detail) => {
        if (!cancelled) setGroups(detail.categories);
      },
      () => {
        // A catalog that will not load costs the group list, not the form:
        // every group is still a valid answer.
        if (!cancelled) setGroups([]);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, chosenId, siteId]);

  const chosen = catalogs.find((catalog) => catalog.id === draft.catalog_id);
  const missingGroup =
    draft.category !== "" && !groups.some((group) => group.slug === draft.category);

  return (
    <>
      <TextField
        label={strings.sitesCatalogSectionHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      {loading ? (
        <div className={styles.collectionFieldLoading} role="status">
          <Spinner size={16} />
          <span>{strings.sitesCatalogsLoading}</span>
        </div>
      ) : catalogs.length === 0 ? (
        <div className={styles.collectionFieldEmpty}>
          <strong>{strings.sitesCatalogSectionNoCatalogs}</strong>
          <span>{strings.sitesCatalogSectionNoCatalogsHint}</span>
          <Button variant="ghost" onClick={() => navigate(`/sites/${siteId}/catalogs`)}>
            {strings.sitesNewCatalog}
          </Button>
        </div>
      ) : (
        <>
          <Field label={strings.sitesCatalogSectionChoose}>
            <select
              className={styles.input}
              value={draft.catalog_id}
              onChange={(event) =>
                // A group handle belongs to the catalog it came from; changing
                // the catalog drops it rather than carrying a handle that
                // means nothing here.
                onChange({ ...draft, catalog_id: event.target.value, category: "" })
              }
            >
              {catalogs.map((catalog) => (
                <option key={catalog.id} value={catalog.id}>
                  {catalog.name}
                </option>
              ))}
            </select>
          </Field>
          <Field
            label={strings.sitesCatalogSectionGroup}
            hint={strings.sitesCatalogSectionGroupHint}
          >
            <select
              className={styles.input}
              value={draft.category}
              onChange={(event) => onChange({ ...draft, category: event.target.value })}
            >
              <option value="">{strings.sitesCatalogSectionAllGroups}</option>
              {groups.map((group) => (
                <option key={group.id} value={group.slug}>
                  {group.name}
                </option>
              ))}
              {missingGroup && (
                <option value={draft.category}>
                  {strings.sitesCatalogSectionGoneGroup(draft.category)}
                </option>
              )}
            </select>
          </Field>
          <p className={styles.hint}>
            {chosen?.ordersEnabled === true
              ? strings.sitesCatalogSectionOrdersOn
              : strings.sitesCatalogSectionOrdersOff}
          </p>
        </>
      )}
      {error !== null && <p className={styles.aiEditError} role="alert">{error}</p>}
    </>
  );
}

/** Offering one of the site's booking services on a page. The section holds a
 *  choice and a heading; the length, the week it is open and the questions a
 *  visitor answers are the service's own and are edited on the Bookings screen,
 *  which this form links to rather than duplicating. Two states are said out
 *  loud because a visitor would otherwise be the one to discover them: a site
 *  with no service yet, and a service that is switched off. */
function BookingFields({ draft, onChange }: { draft: BookingDraft; onChange: Change }) {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [bookings, setBookings] = useState<SiteBooking[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void api
      .bookings(siteId)
      .then(
        (stored) => {
          if (cancelled) return;
          setBookings(stored);
          setError(null);
        },
        (reason: unknown) => {
          if (!cancelled) setError(sitesMessage(reason, strings.sitesBookingsLoadFailed));
        },
      )
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, siteId]);

  const firstBookingId = bookings[0]?.id;
  useEffect(() => {
    if (draft.booking_id === "" && firstBookingId !== undefined) {
      onChange({ ...draft, booking_id: firstBookingId });
    }
  }, [draft, firstBookingId, onChange]);

  const chosen = bookings.find((booking) => booking.id === draft.booking_id);
  return (
    <>
      <TextField
        label={strings.sitesBookingSectionHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      {loading ? (
        <div className={styles.collectionFieldLoading} role="status">
          <Spinner size={16} />
          <span>{strings.sitesBookingsLoading}</span>
        </div>
      ) : bookings.length === 0 ? (
        <div className={styles.collectionFieldEmpty}>
          <strong>{strings.sitesBookingSectionNoServices}</strong>
          <span>{strings.sitesBookingSectionNoServicesHint}</span>
          <Button variant="ghost" onClick={() => navigate(`/sites/${siteId}/bookings`)}>
            {strings.sitesNewBooking}
          </Button>
        </div>
      ) : (
        <>
          <Field label={strings.sitesBookingSectionChoose}>
            <select
              className={styles.input}
              value={draft.booking_id}
              onChange={(event) => onChange({ ...draft, booking_id: event.target.value })}
            >
              {bookings.map((booking) => (
                <option key={booking.id} value={booking.id}>
                  {booking.active
                    ? booking.name
                    : strings.sitesBookingSectionOffOption(booking.name)}
                </option>
              ))}
            </select>
          </Field>
          <p className={styles.hint}>
            {chosen === undefined
              ? strings.sitesBookingSectionGone
              : chosen.active
                ? strings.sitesBookingSectionLength(chosen.durationMinutes)
                : strings.sitesBookingSectionOff}
          </p>
        </>
      )}
      {error !== null && <p className={styles.aiEditError} role="alert">{error}</p>}
    </>
  );
}

/** The ticket shop's door on a page. The section carries the words above the
 *  link and nothing else; the events, their prices and their seats live on
 *  the Tickets screen, which this form links to rather than duplicating. A
 *  site with nothing on sale yet is told so here — a visitor must never be
 *  the one to discover an empty shop. */
function TicketsFields({ draft, onChange }: { draft: TicketsDraft; onChange: Change }) {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [onSale, setOnSale] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api.ticketEvents(siteId).then(
      (stored) => {
        if (!cancelled) setOnSale(stored.events.length);
      },
      () => {
        // A list that will not load costs the hint, not the form: the words
        // above the link are editable regardless.
        if (!cancelled) setOnSale(null);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, siteId]);

  return (
    <>
      <TextField
        label={strings.sitesTicketSectionHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesTicketSectionBody}
        value={draft.body}
        onChange={(body) => onChange({ ...draft, body })}
        copyPointer="/body"
      />
      {onSale === 0 ? (
        <div className={styles.collectionFieldEmpty}>
          <strong>{strings.sitesTicketSectionNoEvents}</strong>
          <span>{strings.sitesTicketSectionNoEventsHint}</span>
          <Button variant="ghost" onClick={() => navigate(`/sites/${siteId}/tickets`)}>
            {strings.sitesTickets}
          </Button>
        </div>
      ) : (
        <p className={styles.hint}>
          {onSale === null
            ? strings.sitesTicketSectionHint
            : strings.sitesTicketSectionOnSale(onSale)}
        </p>
      )}
    </>
  );
}

/** The stock shop's door on a page — the tickets form made again for goods
 *  on a shelf. The section carries the words above the link and nothing
 *  else; the shelf, its prices and its stock live on the Shop screen, which
 *  this form links to rather than duplicating. A site with an empty shelf is
 *  told so here — a visitor must never be the one to discover an empty
 *  shop. */
function ShopFields({ draft, onChange }: { draft: ShopDraft; onChange: Change }) {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [listed, setListed] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api.shopItems(siteId).then(
      (stored) => {
        if (!cancelled) setListed(stored.items.length);
      },
      () => {
        // A list that will not load costs the hint, not the form: the words
        // above the link are editable regardless.
        if (!cancelled) setListed(null);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, siteId]);

  return (
    <>
      <TextField
        label={strings.sitesShopSectionHeading}
        value={draft.heading}
        onChange={(heading) => onChange({ ...draft, heading })}
        autoFocus
        copyPointer="/heading"
      />
      <LongTextField
        label={strings.sitesShopSectionBody}
        value={draft.body}
        onChange={(body) => onChange({ ...draft, body })}
        copyPointer="/body"
      />
      {listed === 0 ? (
        <div className={styles.collectionFieldEmpty}>
          <strong>{strings.sitesShopSectionNoItems}</strong>
          <span>{strings.sitesShopSectionNoItemsHint}</span>
          <Button variant="ghost" onClick={() => navigate(`/sites/${siteId}/shop`)}>
            {strings.sitesShop}
          </Button>
        </div>
      ) : (
        <p className={styles.hint}>
          {listed === null
            ? strings.sitesShopSectionHint
            : strings.sitesShopSectionListed(listed)}
        </p>
      )}
    </>
  );
}

function FooterFields({ draft, onChange }: { draft: FooterDraft; onChange: Change }) {
  return (
    <>
      <TextField
        label={strings.sitesFieldFooterText}
        value={draft.text}
        onChange={(text) => onChange({ ...draft, text })}
        autoFocus
        copyPointer="/text"
      />
      <ItemsEditor
        addLabel={strings.sitesAddLink}
        items={draft.links}
        onChange={(links) => onChange({ ...draft, links })}
        blank={blankLink}
        render={(link, update) => <LinkFields value={link} onChange={update} />}
      />
    </>
  );
}

function FormFields({ draft, onChange }: { draft: SectionDraft; onChange: Change }) {
  switch (draft.type) {
    case "nav":
      return <NavFields draft={draft} onChange={onChange} />;
    case "hero":
      return <HeroFields draft={draft} onChange={onChange} />;
    case "features":
      return <FeaturesFields draft={draft} onChange={onChange} />;
    case "text_image":
      return <TextImageFields draft={draft} onChange={onChange} />;
    case "gallery":
      return <GalleryFields draft={draft} onChange={onChange} />;
    case "testimonials":
      return <TestimonialsFields draft={draft} onChange={onChange} />;
    case "pricing":
      return <PricingFields draft={draft} onChange={onChange} />;
    case "team":
      return <TeamFields draft={draft} onChange={onChange} />;
    case "faq":
      return <FaqFields draft={draft} onChange={onChange} />;
    case "cta":
      return <CtaFields draft={draft} onChange={onChange} />;
    case "contact_form":
      return <ContactFormFields draft={draft} onChange={onChange} />;
    case "collection":
      return <CollectionFields draft={draft} onChange={onChange} />;
    case "catalog":
      return <CatalogFields draft={draft} onChange={onChange} />;
    case "booking":
      return <BookingFields draft={draft} onChange={onChange} />;
    case "tickets":
      return <TicketsFields draft={draft} onChange={onChange} />;
    case "shop":
      return <ShopFields draft={draft} onChange={onChange} />;
    case "custom_code":
      // No copy tools anywhere in this form: the assistant refuses to write
      // or change code by name (`alo-ai`'s sites module), so offering the
      // affordance would only produce a refusal.
      return <CustomCodeFields draft={draft} onChange={onChange} />;
    case "footer":
      return <FooterFields draft={draft} onChange={onChange} />;
  }
}

// ---- the dialog -------------------------------------------------------------

/** The three sections that name something else of the site cannot be saved
 *  until they name it: a section pointing at nothing would be a page that
 *  publishes an empty hole, or refuses the publish outright. Every other kind
 *  is ruled on by the server alone. */
function canSubmit(draft: SectionDraft): boolean {
  switch (draft.type) {
    case "collection":
      return draft.collection_id !== "";
    case "catalog":
      return draft.catalog_id !== "";
    case "booking":
      return draft.booking_id !== "";
    default:
      return true;
  }
}

/** The section prop form: fresh for a kind picked in the picker, prefilled
 *  when editing an existing section. Saving hands the wire section up; the
 *  caller talks to the server and feeds any refusal back through `error`,
 *  so the dialog stays open with everything the user typed. */
export function SectionFormDialog({
  kind,
  initial,
  busy,
  error,
  onClose,
  onSave,
  copyContext,
}: {
  kind: SectionKind;
  /** The stored section when editing; absent when adding. */
  initial?: Section | undefined;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onSave: (section: Section) => void;
  /** Present only for a stored section. New sections have no stable page
   *  target yet, so their copy remains directly editable until first save. */
  copyContext?: CopyContextValue | undefined;
}) {
  const [draft, setDraft] = useState<SectionDraft>(() => toDraft(kind, initial));
  const label = kindLabel(kind);
  return (
    <DialogFrame
      Icon={Blocks}
      title={
        initial === undefined
          ? strings.sitesAddSectionTitle(label)
          : strings.sitesEditSectionTitle(label)
      }
      subtitle={kindDescription(kind)}
      error={error}
      busy={busy}
      canSubmit={canSubmit(draft)}
      submitLabel={strings.sitesSaveSection}
      onClose={onClose}
      onSubmit={() => onSave(toSection(draft))}
    >
      <CopyContext.Provider value={copyContext ?? null}>
        <FormFields draft={draft} onChange={setDraft} />
      </CopyContext.Provider>
    </DialogFrame>
  );
}
