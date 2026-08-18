// The letter as one person will receive it (alo Campaigns, ADR 0044, wave
// C3.6) — and the sentence that says what a preview is not.
//
// **Everything drawn here came from the server.** The HTML, the plain-text
// alternative and the merge-field report are one compilation, the same one a
// send would use. A browser that re-rendered any part of it would be a second
// opinion about what a customer's customers receive, and the two would diverge
// on the day somebody changed a renderer.
//
// Three things on this screen are the interface laws rather than taste:
//
// - **The caveat is not fine print.** Our renderer's output is not proof of how
//   Outlook 2016 will draw it; Word's engine is. Saying so plainly, in the
//   place where somebody decides a letter is ready, is what makes the test copy
//   in Drafts the obvious next step rather than a feature nobody found.
// - **"Show as" offers the copy nobody proof-reads.** Most of an audience built
//   from web forms has no name on file, so the fallback copy is the mail most
//   people get. It is one press away and named in plain words, because a writer
//   who has only read the personalised version has not read their own letter.
// - **The field table says whose words each value is.** "Hi Jean," and "Hi
//   there," look equally finished. The only way to see which is which is to be
//   told, so the source column is the point of the table, not decoration.
import { useState } from "react";
import { Mail } from "lucide-react";

import { Badge, Button, Field, Select, Spinner, Table, TableEmpty, Td, Th } from "../ds";
import { strings } from "../i18n";
import { campaignsMessage, useCampaignsApi } from "./api";
import { mergeFieldLabel, previewAgainstLabel } from "./format";
import { useLetters } from "./useLetters";
import { useMergeFields } from "./useMergeFields";
import { PREVIEW_AS_FALLBACKS } from "./types";
import styles from "./CampaignsModule.module.css";

/** Which part of the letter is on screen. Both are real: half of recipients
 *  read the text one, and a filter reads it before anybody does. */
type Part = "html" | "text";

export function LetterPreview() {
  const api = useCampaignsApi();
  const letters = useLetters();
  const vocabulary = useMergeFields();
  const [part, setPart] = useState<Part>("html");
  const [testing, setTesting] = useState(false);
  const [wrote, setWrote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const preview = letters.preview;

  function putACopyInDrafts() {
    if (letters.openLetter === "") return;
    setTesting(true);
    setWrote(null);
    void (async () => {
      try {
        const { draft } = await api.testDraft(letters.openLetter, letters.showAs);
        setError(null);
        setWrote(draft.to);
      } catch (err) {
        setError(campaignsMessage(err, strings.campaignsTestDraftFailed));
      } finally {
        setTesting(false);
      }
    })();
  }

  const banner = error ?? letters.error;

  if (!letters.loading && letters.letters.length === 0) {
    return (
      <div className={styles.empty}>
        <span className={styles.emptyArt} aria-hidden="true">
          <Mail size={38} />
        </span>
        <h2 className={styles.emptyTitle}>{strings.campaignsNoLettersTitle}</h2>
        <p className={styles.emptyBody}>{strings.campaignsNoLettersBody}</p>
      </div>
    );
  }

  return (
    <div className={styles.list}>
      {banner !== null && (
        <p className={styles.error} role="alert">
          {banner}
        </p>
      )}

      <div className={styles.question}>
        <Field label={strings.campaignsLetterLabel}>
          {(control) => (
            <Select
              {...control}
              value={letters.openLetter}
              onChange={(e) => letters.open(e.target.value)}
            >
              {letters.letters.map((letter) => (
                <option key={letter.id} value={letter.id}>
                  {letter.subject}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <Field label={strings.campaignsShowAsLabel} hint={strings.campaignsShowAsHint}>
          {(control) => (
            <Select
              {...control}
              value={letters.showAs}
              onChange={(e) => letters.setShowAs(e.target.value)}
            >
              <option value="">{strings.campaignsShowAsRecipient}</option>
              <option value={PREVIEW_AS_FALLBACKS}>{strings.campaignsShowAsFallbacks}</option>
            </Select>
          )}
        </Field>
        <Field label={strings.campaignsPartLabel} hint={strings.campaignsPartHint}>
          {(control) => (
            <Select {...control} value={part} onChange={(e) => setPart(e.target.value as Part)}>
              <option value="html">{strings.campaignsPartHtml}</option>
              <option value="text">{strings.campaignsPartText}</option>
            </Select>
          )}
        </Field>
      </div>

      {letters.loading && preview === null ? (
        <div className={styles.previewLoading}>
          <Spinner />
        </div>
      ) : preview === null ? null : (
        <>
          <p className={styles.against}>{previewAgainstLabel(preview.against)}</p>

          {/* The inbox line: the subject and the preview text beside it are what
              a recipient decides on before the letter is ever opened. */}
          <dl className={styles.inboxLine}>
            <dt>{strings.campaignsPreviewSubject}</dt>
            <dd>{preview.subject}</dd>
            <dt>{strings.campaignsPreviewPreheader}</dt>
            <dd>{preview.preheader ?? strings.campaignsPreviewNoPreheader}</dd>
          </dl>

          {part === "html" ? (
            /* `sandbox=""` with no allowances: the letter is a customer's own
               markup and it runs nothing, reaches nothing and navigates
               nothing from inside this app. */
            <iframe
              className={styles.previewFrame}
              title={strings.campaignsPreviewFrameLabel}
              sandbox=""
              srcDoc={preview.html}
            />
          ) : (
            <pre className={styles.previewText}>{preview.text}</pre>
          )}

          <p className={styles.footnote}>{strings.campaignsPreviewCaveat}</p>

          <div className={styles.testRow}>
            <Button variant="secondary" onClick={putACopyInDrafts} disabled={testing}>
              {testing ? <Spinner /> : strings.campaignsTestDraft}
            </Button>
            {wrote !== null && (
              <p className={styles.testDone} role="status">
                {strings.campaignsTestDraftDone(wrote)}
              </p>
            )}
          </div>

          <h2 className={styles.sectionTitle}>{strings.campaignsFieldsTitle}</h2>
          <Table label={strings.campaignsFieldsTitle}>
            <thead>
              <tr>
                <Th>{strings.campaignsColField}</Th>
                <Th>{strings.campaignsColPrinted}</Th>
                <Th>{strings.campaignsColWhoseWords}</Th>
              </tr>
            </thead>
            <tbody>
              {preview.fields.length === 0 ? (
                <TableEmpty cols={3}>{strings.campaignsNoFields}</TableEmpty>
              ) : (
                preview.fields.map((used, index) => (
                  <tr key={`${used.field}-${used.value}-${index}`}>
                    <Td>{mergeFieldLabel(used.field)}</Td>
                    <Td>{used.value}</Td>
                    <Td>
                      {used.fellBack ? (
                        <Badge tone="neutral">{strings.campaignsFieldFallback}</Badge>
                      ) : (
                        <Badge tone="success">{strings.campaignsFieldTheirs}</Badge>
                      )}
                    </Td>
                  </tr>
                ))
              )}
            </tbody>
          </Table>

          {/* Recognition over recall: the vocabulary is on the screen where a
              letter is judged, so nobody has to remember it — and it is read
              from the server, so a field added there appears here without a
              web release. Every one carries its bar, because a field without a
              fallback is what "Hi ," is made of. */}
          {vocabulary.length > 0 && (
            <section className={styles.vocabulary}>
              <h2 className={styles.sectionTitle}>{strings.campaignsVocabularyTitle}</h2>
              <ul className={styles.vocabularyList}>
                {vocabulary.map((field) => (
                  <li key={field}>
                    <code>{strings.campaignsFieldExample(field)}</code>
                    <span>{mergeFieldLabel(field)}</span>
                  </li>
                ))}
              </ul>
              <p className={styles.footnote}>{strings.campaignsVocabularyHint}</p>
            </section>
          )}
        </>
      )}
    </div>
  );
}
