// Everything a section says about one picture: which file it is, what it
// shows (the description a screen reader reads), and how it is framed.
//
// It was part of `SectionForm.tsx` while a picture was only a blob id and a
// line of alt text. S2.07c gave it a crop, a focal point, a deliberate
// "decorative" state and an AI draft of the description — four reasons to
// change that have nothing to do with the other twelve section forms, so it
// moved out with them.
//
// The description gets more room here than the upload does, on purpose: an
// undescribed picture is invisible to somebody using a screen reader, and the
// form says so rather than leaving a blank field to be scrolled past.
import { useRef, useState } from "react";
import { Sparkles, Upload } from "lucide-react";
import { useParams } from "react-router-dom";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { Button } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { useCopyContext } from "./copyContext";
import { useImageSource } from "./imageSource";
import { ImageFraming } from "./ImageFraming";
import { Field } from "./parts";
import type { SectionImage } from "./sections";
import type { SiteEditEnvelope } from "./types";
import styles from "./SitesModule.module.css";

/**
 * "Suggest a description" — a proposal, never a write.
 *
 * The model has not seen the photograph (nothing in this build shows it one),
 * so the draft comes from the words already in this section, the tool says so
 * in as many words, and the sentence appears next to the picture itself so
 * the one party who can see it is the one who approves it. Approving applies
 * the guarded operation server-side, exactly like the copy tools.
 */
function AltTextTool({ pointer, value }: { pointer: string; value: string }) {
  const context = useCopyContext();
  const api = useSitesApi();
  const [proposed, setProposed] = useState<string | null>(null);
  const [proposal, setProposal] = useState<SiteEditEnvelope | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (context === null) return null;

  async function propose() {
    if (context === null) return;
    setBusy(true);
    setError(null);
    try {
      const prepared = await api.proposePageCopyEdit(context.siteId, context.pageId, {
        target: context.target,
        pointer: `${pointer}/alt`,
        action: "alt_text",
      });
      const operation = prepared.proposal.operations[0];
      if (operation?.op !== "rewrite_copy") throw new Error(strings.sitesAiAltFailed);
      setProposal(prepared.proposal);
      setProposed(operation.text);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesAiAltFailed));
    } finally {
      setBusy(false);
    }
  }

  async function approve() {
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
        size="sm"
        icon={<Sparkles size={14} />}
        disabled={busy}
        onClick={() => void propose()}
      >
        {value.trim() === "" ? strings.sitesAiAltWrite : strings.sitesAiAltImprove}
      </Button>
      {proposed !== null && (
        <div className={styles.copyProposal} aria-live="polite">
          <div>
            <span>{strings.sitesAiAltProposed}</span>
            <p>{proposed}</p>
          </div>
          <p className={styles.copyProposalHint}>{strings.sitesAiAltUnseen}</p>
          <div className={styles.copyProposalActions}>
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() => {
                setProposal(null);
                setProposed(null);
              }}
            >
              {strings.sitesAiDiscard}
            </Button>
            <Button disabled={busy} onClick={() => void approve()}>
              {busy ? strings.sitesAiApplying : strings.sitesAiApprove}
            </Button>
          </div>
        </div>
      )}
      {error !== null && (
        <p className={styles.aiEditError} role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

/**
 * An image's inputs: the upload button (the picture goes through Drive and
 * the field takes its blob id), the id itself for pasting or clearing, how it
 * is framed, and what it shows. A blank id means "no image" for optional
 * slots, and hides everything that would be describing nothing.
 *
 * `pointer` is the image's JSON pointer inside the section (`/image`,
 * `/images/2`); it is what lets the description tool name the exact field,
 * and its absence simply means no AI draft is offered.
 */
export function ImageFields({
  legend,
  value,
  onChange,
  pointer,
}: {
  legend?: string;
  value: SectionImage;
  onChange: (patch: Partial<SectionImage>) => void;
  pointer?: string | undefined;
}) {
  const jmap = useJmapClient();
  const { siteId = "" } = useParams();
  const fileInput = useRef<HTMLInputElement>(null);
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const source = useImageSource(siteId, value.blob_id);
  const chosen = value.blob_id.trim() !== "";
  const decorative = value.decorative === true;

  function upload(file: File) {
    setUploading(true);
    setUploadError(null);
    jmap.driveUploadBlob(null, null, file).then(
      ({ blobId }) => {
        // A new picture is not the old one: its frame would be a rectangle of
        // a different photograph, so framing starts over.
        onChange({ blob_id: blobId, crop: undefined, focal: undefined });
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
            onChange={(e) => onChange({ blob_id: e.target.value, crop: undefined, focal: undefined })}
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
      {uploadError !== null && (
        <p className={styles.hint} role="alert">
          {uploadError}
        </p>
      )}
      {chosen && <ImageFraming value={value} url={source} onChange={onChange} />}
      <Field label={strings.sitesFieldImageAlt} hint={strings.sitesImageAltHint}>
        <input
          className={styles.input}
          value={value.alt}
          disabled={decorative}
          onChange={(e) => onChange({ alt: e.target.value })}
        />
      </Field>
      {chosen && !decorative && value.alt.trim() === "" && (
        <p className={styles.hint} role="status">
          {strings.sitesImageAltMissing}
        </p>
      )}
      {chosen && pointer !== undefined && !decorative && (
        <AltTextTool pointer={pointer} value={value.alt} />
      )}
      <label className={styles.toggle}>
        <input
          type="checkbox"
          checked={decorative}
          onChange={(e) =>
            // A decorative picture has nothing to describe, and the schema
            // refuses one that still carries a description — so the two move
            // together rather than saving into a refusal.
            onChange(e.target.checked ? { decorative: true, alt: "" } : { decorative: false })
          }
        />
        {strings.sitesImageDecorative}
      </label>
      <p className={styles.hint}>{strings.sitesImageDecorativeHint}</p>
    </fieldset>
  );
}
