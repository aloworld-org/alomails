import { useEffect, useState } from "react";
import { FileText, Plus, Trash2 } from "lucide-react";

import {
  Button,
  Card,
  Chip,
  Field,
  IconButton,
  Input,
  Spinner,
  useDialogs,
} from "../ds";
import { strings } from "../i18n";
import { hrMessage, useHrApi } from "./api";
import { DialogFrame, EmptyState, ErrorBanner } from "./parts";
import type { HrLetterTemplate, HrLetterTemplateDraft } from "./types";
import styles from "./hr.module.css";

const BLANK: HrLetterTemplateDraft = { name: "", subject: "", body: "" };

export function LetterTemplatesView() {
  const api = useHrApi();
  const dialogs = useDialogs();
  const [templates, setTemplates] = useState<HrLetterTemplate[] | null>(null);
  const [fields, setFields] = useState<string[]>([]);
  const [editing, setEditing] = useState<HrLetterTemplate | "new" | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void api
      .letterTemplates()
      .then((catalog) => {
        if (live) {
          setTemplates(catalog.templates);
          setFields(catalog.fields);
        }
      })
      .catch((error) => {
        if (live) setProblem(hrMessage(error, strings.hrTemplatesLoadFailed));
      });
    return () => {
      live = false;
    };
  }, [api]);

  async function remove(template: HrLetterTemplate) {
    const confirmed = await dialogs.confirm({
      title: strings.hrTemplateDeleteTitle(template.name),
      message: strings.hrTemplateDeleteBody,
      confirmLabel: strings.hrTemplateDelete,
      danger: true,
    });
    if (!confirmed) return;
    try {
      await api.deleteLetterTemplate(template.id);
      setTemplates(
        (current) => current?.filter((item) => item.id !== template.id) ?? [],
      );
    } catch (error) {
      setProblem(hrMessage(error, strings.hrTemplateDeleteFailed));
    }
  }

  return (
    <section className={styles.page}>
      <div className={styles.templatesHead}>
        <div>
          <h2>{strings.hrTemplatesTitle}</h2>
          <p>{strings.hrTemplatesIntro}</p>
        </div>
        <Button icon={<Plus size={17} />} onClick={() => setEditing("new")}>
          {strings.hrTemplateNew}
        </Button>
      </div>
      {problem !== null && <ErrorBanner message={problem} />}
      {templates === null && problem === null ? (
        <div className={styles.center}>
          <Spinner />
        </div>
      ) : null}
      {templates?.length === 0 ? (
        <EmptyState
          Icon={FileText}
          title={strings.hrTemplatesEmpty}
          body={strings.hrTemplatesEmptyBody}
          cta={strings.hrTemplateNew}
          onCta={() => setEditing("new")}
        />
      ) : (
        <ul className={styles.templateGrid}>
          {templates?.map((template) => (
            // The surface is a `ds/Card` laying out its own regions: the whole
            // face opens the template, so the padding belongs to the button
            // inside it rather than to the card — which is also what lets the
            // delete control reach the corner.
            <Card
              as="li"
              pad="none"
              key={template.id}
              interactive
              className="relative"
            >
              <button
                className={styles.templateOpen}
                type="button"
                onClick={() => setEditing(template)}
              >
                <FileText size={20} />
                <strong>{template.name}</strong>
                <span>{template.subject}</span>
                <small>
                  {strings.hrTemplateFields(template.fields.length)}
                </small>
              </button>
              <span className="absolute top-3 right-3">
                <IconButton
                  label={strings.hrTemplateDeleteTitle(template.name)}
                  icon={<Trash2 size={17} />}
                  onClick={() => void remove(template)}
                />
              </span>
            </Card>
          ))}
        </ul>
      )}
      {editing !== null && (
        <TemplateEditor
          template={editing === "new" ? null : editing}
          fields={fields}
          onClose={() => setEditing(null)}
          onSaved={(saved) => {
            setTemplates((current) =>
              editing === "new"
                ? [saved, ...(current ?? [])]
                : (current ?? []).map((item) =>
                    item.id === saved.id ? saved : item,
                  ),
            );
            setEditing(null);
          }}
        />
      )}
    </section>
  );
}

function TemplateEditor({
  template,
  fields,
  onClose,
  onSaved,
}: {
  template: HrLetterTemplate | null;
  fields: string[];
  onClose: () => void;
  onSaved: (template: HrLetterTemplate) => void;
}) {
  const api = useHrApi();
  const [draft, setDraft] = useState<HrLetterTemplateDraft>(template ?? BLANK);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const save = async () => {
    setBusy(true);
    setProblem(null);
    try {
      onSaved(
        template === null
          ? await api.createLetterTemplate(draft)
          : await api.updateLetterTemplate(template.id, draft),
      );
    } catch (error) {
      setProblem(hrMessage(error, strings.hrTemplateSaveFailed));
      setBusy(false);
    }
  };
  return (
    <DialogFrame
      Icon={FileText}
      title={
        template === null
          ? strings.hrTemplateCreateTitle
          : strings.hrTemplateEditTitle
      }
      subtitle={strings.hrTemplateEditorIntro}
      error={problem}
      busy={busy}
      canSubmit={
        draft.name.trim() !== "" &&
        draft.subject.trim() !== "" &&
        draft.body.trim() !== ""
      }
      submitLabel={strings.hrTemplateSave}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={strings.hrTemplateName}>
        {(control) => (
          <Input
            {...control}
            value={draft.name}
            onChange={(event) =>
              setDraft({ ...draft, name: event.target.value })
            }
          />
        )}
      </Field>
      <Field label={strings.hrTemplateSubject}>
        {(control) => (
          <Input
            {...control}
            value={draft.subject}
            onChange={(event) =>
              setDraft({ ...draft, subject: event.target.value })
            }
          />
        )}
      </Field>
      {/* `ds/` still has no multi-line control, so the letter itself is drawn
          locally — the one box on this screen that is not the design
          system's. */}
      <Field label={strings.hrTemplateBody} hint={strings.hrTemplateBodyHint}>
        {(control) => (
          <textarea
            id={control.id}
            aria-describedby={control["aria-describedby"]}
            className={styles.templateBody}
            value={draft.body}
            onChange={(event) =>
              setDraft({ ...draft, body: event.target.value })
            }
          />
        )}
      </Field>
      <div className={styles.fieldPicker}>
        <span>{strings.hrTemplateInsertField}</span>
        <div>
          {/* Each of these is pressed — it writes a placeholder into the letter
              — so they are `ds/Chip`s rather than words drawn to look like
              one. */}
          {fields.map((field) => (
            <Chip
              key={field}
              tone="accent"
              onClick={() =>
                setDraft({
                  ...draft,
                  body: `${draft.body}${draft.body.endsWith(" ") || draft.body === "" ? "" : " "}{{${field}}}`,
                })
              }
            >
              {field}
            </Chip>
          ))}
        </div>
      </div>
    </DialogFrame>
  );
}
