// The per-type prop forms — one small form per section kind, sharing the same
// few field primitives, in the module's dialog chrome. The form edits a
// draft (`sectionDrafts.ts`) and hands the wire section up on save; the
// SERVER rules on content (blank required text, bad hrefs, empty lists) and
// its 422 sentence is shown here verbatim, so there is exactly one copy of
// every rule.
import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Blocks, Plus, Sparkles, Trash2, Upload } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
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
} from "./sectionDrafts";
import type { Section, SectionImage, SectionKind, SectionLink } from "./sections";
import type { SectionsEnvelope } from "./sections";
import type { SiteCopyAction, SiteEditEnvelope, SiteEditTarget } from "./types";
import type { SiteCollection } from "./types";
import { sitesMessage, useSitesApi } from "./api";
import { DialogFrame, Field } from "./parts";
import styles from "./SitesModule.module.css";

// ---- field primitives -------------------------------------------------------

interface CopyContextValue {
  siteId: string;
  pageId: string;
  target: SiteEditTarget;
  onApplied: (sections: SectionsEnvelope) => void;
}

const CopyContext = createContext<CopyContextValue | null>(null);

function CopyTools({ pointer, value }: { pointer: string; value: string }) {
  const context = useContext(CopyContext);
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

/** An image's inputs: an upload button (the picture goes through Drive and
 *  the field takes its blob id), the id itself for pasting/clearing, and the
 *  alt text. A blank id means "no image" for optional slots. */
function ImageFields({
  legend,
  value,
  onChange,
}: {
  legend?: string;
  value: SectionImage;
  onChange: (patch: Partial<SectionImage>) => void;
}) {
  const jmap = useJmapClient();
  const fileInput = useRef<HTMLInputElement>(null);
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);

  function upload(file: File) {
    setUploading(true);
    setUploadError(null);
    jmap.driveUploadBlob(null, null, file).then(
      ({ blobId }) => {
        onChange({ blob_id: blobId });
        setUploading(false);
      },
      () => {
        setUploadError(strings.sitesUploadFailed);
        setUploading(false);
      },
    );
  }

  return (
    <fieldset className={styles.subGroup}>
      {legend !== undefined && <legend className={styles.subLegend}>{legend}</legend>}
      <Field label={strings.sitesFieldImageId} hint={strings.sitesImageIdHint}>
        <div className={styles.uploadRow}>
          <input
            className={`${styles.input} ${styles.mono}`}
            value={value.blob_id}
            onChange={(e) => onChange({ blob_id: e.target.value })}
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
          />
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            hidden
            onChange={(e) => {
              const file = e.target.files?.[0];
              e.target.value = "";
              if (file !== undefined) upload(file);
            }}
          />
          <Button
            variant="ghost"
            size="sm"
            icon={<Upload size={14} />}
            disabled={uploading}
            onClick={() => fileInput.current?.click()}
          >
            {strings.sitesUploadImage}
          </Button>
        </div>
      </Field>
      {uploadError !== null && <p className={styles.hint} role="alert">{uploadError}</p>}
      <Field label={strings.sitesFieldImageAlt} hint={strings.sitesImageAltHint}>
        <input
          className={styles.input}
          value={value.alt}
          onChange={(e) => onChange({ alt: e.target.value })}
        />
      </Field>
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
        render={(image, update) => <ImageFields value={image} onChange={update} />}
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
    case "footer":
      return <FooterFields draft={draft} onChange={onChange} />;
  }
}

// ---- the dialog -------------------------------------------------------------

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
      canSubmit={draft.type !== "collection" || draft.collection_id !== ""}
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
