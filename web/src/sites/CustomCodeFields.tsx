// The custom-code block's editor: three code fields, the capabilities the
// sandboxed frame is granted, how tall it is — and, above all of it, the
// boundary written out in plain words, because a block of code on a public
// page is the one section where what the browser will NOT let it do is the
// thing the author most needs to know.
//
// Nothing here validates. The server owns every rule (`site_custom_code.rs`)
// and its 422 sentence is shown verbatim by the dialog around this form. The
// byte counters below are the one place a cap is repeated, and they only
// COUNT: they never block a save, so a cap that moves in Rust makes a counter
// stale, never a save impossible.
import { ShieldCheck } from "lucide-react";

import { strings } from "../i18n";
import { Field } from "./parts";
import type { CustomCodeDraft, SectionDraft } from "./sectionDrafts";
import styles from "./SitesModule.module.css";

/** The caps as `platform/alo-store/src/site_custom_code.rs` states them, for
 *  the counters alone — a person cannot count bytes, and finding out at save
 *  time that a snippet is 400 bytes too long is a bad way to learn. */
const MAX_HTML_BYTES = 16_384;
const MAX_CSS_BYTES = 8_192;
const MAX_JS_BYTES = 8_192;
const MAX_TOTAL_BYTES = 24_576;
/** The frame-height bounds, same source. */
const MIN_HEIGHT_PX = 40;
const MAX_HEIGHT_PX = 2_000;

/** The size the wire carries — bytes, not characters, exactly as the cap is
 *  expressed. */
function byteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

/** One part of the block: a monospaced editor with its own budget under it. */
function CodeField({
  label,
  hint,
  value,
  maxBytes,
  rows,
  onChange,
}: {
  label: string;
  hint: string;
  value: string;
  maxBytes: number;
  rows: number;
  onChange: (value: string) => void;
}) {
  const used = byteLength(value);
  const over = used > maxBytes;
  // Deliberately not `Field`: the budget belongs to this control but must sit
  // OUTSIDE its `<label>`, or the editor's accessible name would grow a byte
  // count that changes on every keystroke.
  return (
    <div className={styles.field}>
      <label className={styles.fieldLabel}>
        <span className={styles.label}>{label}</span>
        <textarea
          className={`${styles.input} ${styles.textarea} ${styles.mono} ${styles.codeArea}`}
          value={value}
          rows={rows}
          spellCheck={false}
          autoCapitalize="none"
          autoCorrect="off"
          onChange={(event) => onChange(event.target.value)}
        />
      </label>
      <span className={styles.hint}>{hint}</span>
      <span className={over ? styles.codeBudgetOver : styles.codeBudget} aria-live="polite">
        {over
          ? strings.sitesCustomCodeBytesOver(used, maxBytes)
          : strings.sitesCustomCodeBytes(used, maxBytes)}
      </span>
    </div>
  );
}

/** The whole form body for a `custom_code` section. */
export function CustomCodeFields({
  draft,
  onChange,
}: {
  draft: CustomCodeDraft;
  onChange: (draft: SectionDraft) => void;
}) {
  const total = byteLength(draft.html) + byteLength(draft.css) +
    (draft.scripts ? byteLength(draft.js) : 0);
  const scriptMissing = draft.scripts && draft.js.trim() === "";
  const scriptDropped = !draft.scripts && draft.js.trim() !== "";
  return (
    <>
      {/* The risk boundary, stated before the first field rather than found
          out later: what the frame stops, and what it does not. */}
      <section className={styles.codeBoundary} aria-labelledby="custom-code-boundary">
        <h3 id="custom-code-boundary" className={styles.codeBoundaryTitle}>
          <ShieldCheck size={16} aria-hidden="true" />
          {strings.sitesCustomCodeBoundaryTitle}
        </h3>
        <ul className={styles.codeBoundaryList}>
          <li>{strings.sitesCustomCodeBoundarySealed}</li>
          <li>{strings.sitesCustomCodeBoundaryNoNetwork}</li>
          <li>{strings.sitesCustomCodeBoundaryYours}</li>
        </ul>
      </section>

      <Field label={strings.sitesFieldHeading} hint={strings.sitesCustomCodeHeadingHint}>
        <input
          className={styles.input}
          value={draft.heading}
          autoFocus
          onChange={(event) => onChange({ ...draft, heading: event.target.value })}
        />
      </Field>
      <Field
        label={strings.sitesCustomCodeFrameTitle}
        hint={strings.sitesCustomCodeFrameTitleHint}
      >
        <input
          className={styles.input}
          value={draft.title}
          onChange={(event) => onChange({ ...draft, title: event.target.value })}
        />
      </Field>

      <CodeField
        label={strings.sitesCustomCodeHtml}
        hint={strings.sitesCustomCodeHtmlHint}
        value={draft.html}
        maxBytes={MAX_HTML_BYTES}
        rows={8}
        onChange={(html) => onChange({ ...draft, html })}
      />
      <CodeField
        label={strings.sitesCustomCodeCss}
        hint={strings.sitesCustomCodeCssHint}
        value={draft.css}
        maxBytes={MAX_CSS_BYTES}
        rows={5}
        onChange={(css) => onChange({ ...draft, css })}
      />

      <fieldset className={styles.subGroup}>
        <legend className={styles.subLegend}>{strings.sitesCustomCodeCapabilities}</legend>
        <p className={styles.hint}>{strings.sitesCustomCodeCapabilitiesHint}</p>

        <label className={styles.toggle}>
          <input
            type="checkbox"
            checked={draft.scripts}
            onChange={(event) => onChange({ ...draft, scripts: event.target.checked })}
          />
          {strings.sitesCustomCodeScripts}
        </label>
        <p className={styles.hint}>{strings.sitesCustomCodeScriptsHint}</p>
        {draft.scripts && (
          <CodeField
            label={strings.sitesCustomCodeJs}
            hint={strings.sitesCustomCodeJsHint}
            value={draft.js}
            maxBytes={MAX_JS_BYTES}
            rows={6}
            onChange={(js) => onChange({ ...draft, js })}
          />
        )}
        {/* Both halves of the server's rule, said before it refuses: a
            capability with nothing behind it, and a script that would be
            dropped because nothing may run it. */}
        {scriptMissing && (
          <p className={styles.codeCapabilityNote}>{strings.sitesCustomCodeScriptMissing}</p>
        )}
        {scriptDropped && (
          <p className={styles.codeCapabilityNote}>{strings.sitesCustomCodeScriptDropped}</p>
        )}

        <label className={styles.toggle}>
          <input
            type="checkbox"
            checked={draft.inline_images}
            onChange={(event) => onChange({ ...draft, inline_images: event.target.checked })}
          />
          {strings.sitesCustomCodeImages}
        </label>
        <p className={styles.hint}>{strings.sitesCustomCodeImagesHint}</p>
      </fieldset>

      <Field label={strings.sitesCustomCodeHeight} hint={strings.sitesCustomCodeHeightHint}>
        <input
          className={styles.input}
          type="number"
          inputMode="numeric"
          min={MIN_HEIGHT_PX}
          max={MAX_HEIGHT_PX}
          step={10}
          value={draft.height}
          onChange={(event) => onChange({ ...draft, height: event.target.value })}
        />
      </Field>

      <p className={total > MAX_TOTAL_BYTES ? styles.codeBudgetOver : styles.codeBudget}>
        {strings.sitesCustomCodeTotalBytes(total, MAX_TOTAL_BYTES)}
      </p>
    </>
  );
}
