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
import {
  createContext,
  useContext,
  useRef,
  useState,
  type DragEvent,
} from "react";
import { Image as ImageIcon, Sparkles, Upload } from "lucide-react";
import { useParams } from "react-router-dom";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { Button, cx } from "../ds";
import { sitesMessage, useSitesApi } from "./api";
import { useCopyContext } from "./copyContext";
import { useImageSource } from "./imageSource";
import { ImageFraming } from "./ImageFraming";
import { Field, InformationTip } from "./parts";
import type { SectionImage } from "./sections";
import type { SiteEditEnvelope } from "./types";
import styles from "./SitesModule.module.css";

/** Lets the owning form prevent a save while Drive is still returning the
 * blob id. Without this handshake a quick Save can persist the section before
 * the uploaded image reaches its draft. */
export const ImageUploadActivityContext = createContext<
  ((delta: 1 | -1) => void) | null
>(null);

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
      const prepared = await api.proposePageCopyEdit(
        context.siteId,
        context.pageId,
        {
          target: context.target,
          pointer: `${pointer}/alt`,
          action: "alt_text",
        },
      );
      const operation = prepared.proposal.operations[0];
      if (operation?.op !== "rewrite_copy")
        throw new Error(strings.sitesAiAltFailed);
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
      context.onApplied(
        await api.applyPageEdit(context.siteId, context.pageId, proposal),
      );
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
        {value.trim() === ""
          ? strings.sitesAiAltWrite
          : strings.sitesAiAltImprove}
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
  bare = false,
}: {
  legend?: string;
  value: SectionImage;
  onChange: (patch: Partial<SectionImage>) => void;
  pointer?: string | undefined;
  bare?: boolean;
}) {
  const jmap = useJmapClient();
  const api = useSitesApi();
  const { siteId = "", pageId = "" } = useParams();
  const fileInput = useRef<HTMLInputElement>(null);
  const [uploading, setUploading] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const reportUploadActivity = useContext(ImageUploadActivityContext);
  const source = useImageSource(siteId, value.blob_id);
  const chosen = value.blob_id.trim() !== "";
  const decorative = value.decorative === true;

  async function upload(file: File) {
    if (!file.type.startsWith("image/")) {
      setUploadError(strings.sitesUploadFailed);
      return;
    }
    setUploading(true);
    reportUploadActivity?.(1);
    setUploadError(null);
    try {
      const { blobId } = await jmap.uploadFile(file);
      await api.attachPageImage(siteId, pageId, {
        blobId,
        filename: file.name,
      });
      // A new picture is not the old one: its frame would be a rectangle of
      // a different photograph, so framing starts over.
      onChange({ blob_id: blobId, crop: undefined, focal: undefined });
    } catch {
      setUploadError(strings.sitesUploadFailed);
    } finally {
      setUploading(false);
      reportUploadActivity?.(-1);
    }
  }

  function dragOver(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    if (!uploading) setDragging(true);
  }

  function drop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragging(false);
    if (uploading) return;
    const file = event.dataTransfer.files[0];
    if (file !== undefined) void upload(file);
  }

  return (
    <fieldset
      className={bare ? "m-0 grid min-w-0 gap-4 border-0 p-0" : styles.subGroup}
    >
      {legend !== undefined && (
        <legend
          className={
            bare ? "mb-3 text-sm font-semibold text-primary" : styles.subLegend
          }
        >
          {legend}
        </legend>
      )}
      <div
        className={cx(
          "overflow-hidden rounded-2xl border border-dashed bg-raised transition-[background-color,border-color,box-shadow]",
          dragging
            ? "border-accent bg-accent-soft shadow-[inset_0_0_0_1px_var(--accent)]"
            : "border-default",
        )}
        onDragEnter={dragOver}
        onDragOver={dragOver}
        onDragLeave={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null))
            setDragging(false);
        }}
        onDrop={drop}
      >
        {chosen && source !== null && (
          <img
            src={source}
            alt=""
            className="h-32 w-full border-b border-subtle bg-surface object-contain"
          />
        )}
        <button
          type="button"
          className={cx(
            "flex w-full flex-col items-center justify-center gap-3 !p-5 text-center text-primary transition-colors hover:bg-surface/60 focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-[-2px] disabled:cursor-not-allowed disabled:opacity-50",
            chosen ? "min-h-24" : "min-h-36",
          )}
          disabled={uploading}
          onClick={() => fileInput.current?.click()}
        >
          <span className="grid size-11 place-items-center rounded-xl border border-subtle bg-surface text-accent shadow-sm">
            {chosen ? <ImageIcon size={20} /> : <Upload size={20} />}
          </span>
          <span className="grid gap-1">
            <strong className="text-sm font-semibold">
              {dragging
                ? strings.sitesThemeDropNow
                : strings.sitesThemeDropTitle}
            </strong>
            <span className="text-sm text-secondary">
              {strings.sitesThemeDropBrowse}
            </span>
          </span>
        </button>
        <input
          ref={fileInput}
          type="file"
          accept="image/*"
          hidden
          onChange={(event) => {
            const file = event.target.files?.[0];
            event.target.value = "";
            if (file !== undefined) void upload(file);
          }}
        />
      </div>
      <Field
        label={strings.sitesFieldImageId}
        hint={strings.sitesImageIdHint}
        hintDisplay="tooltip"
      >
        <input
          className={`${styles.input} ${styles.mono}`}
          value={value.blob_id}
          onChange={(event) =>
            onChange({
              blob_id: event.target.value,
              crop: undefined,
              focal: undefined,
            })
          }
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
        />
      </Field>
      {uploadError !== null && (
        <p className={styles.hint} role="alert">
          {uploadError}
        </p>
      )}
      {chosen && (
        <ImageFraming value={value} url={source} onChange={onChange} />
      )}
      <Field
        label={strings.sitesFieldImageAlt}
        hint={strings.sitesImageAltHint}
        hintDisplay="tooltip"
      >
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
      <div className="flex items-center gap-1.5">
        <label className={styles.toggle}>
          <input
            type="checkbox"
            checked={decorative}
            onChange={(e) =>
              // A decorative picture has nothing to describe, and the schema
              // refuses one that still carries a description — so the two move
              // together rather than saving into a refusal.
              onChange(
                e.target.checked
                  ? { decorative: true, alt: "" }
                  : { decorative: false },
              )
            }
          />
          {strings.sitesImageDecorative}
        </label>
        <InformationTip
          label={strings.sitesImageDecorative}
          hint={strings.sitesImageDecorativeHint}
        />
      </div>
    </fieldset>
  );
}
