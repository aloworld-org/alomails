// The address book, opened from the account menu. Two panes: a searchable
// contact list on the left, and the selected contact's editable detail on
// the right. Create, edit, and delete are wired straight to the JMAP
// Contact API (Contact/get + Contact/set). Kept as a modal (like Settings)
// so it needs no route — no Caddy prefix to collide with the API.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Download, Mail, Phone, Plus, Search, Trash2, Upload, UserPlus, X } from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner, cx, useDialogs } from "../ds";
import { useJmapClient } from "../jmap";
import type { Contact, ContactDraft, ContactField } from "../jmap";
import styles from "./ContactsModal.module.css";

interface ContactsModalProps {
  onClose: () => void;
}

/** The editable form state (a superset of a draft, with array fields always
 * present so the inputs are controlled). */
export interface FormState {
  firstName: string;
  lastName: string;
  displayName: string;
  emails: ContactField[];
  phones: ContactField[];
  organization: string;
  jobTitle: string;
  notes: string;
}

const EMPTY: FormState = {
  firstName: "",
  lastName: "",
  displayName: "",
  emails: [{ kind: null, value: "" }],
  phones: [],
  organization: "",
  jobTitle: "",
  notes: "",
};

export function toForm(c: Contact): FormState {
  return {
    firstName: c.firstName ?? "",
    lastName: c.lastName ?? "",
    displayName: c.name,
    emails: c.emails.length > 0 ? c.emails.map((e) => ({ ...e })) : [{ kind: null, value: "" }],
    phones: c.phones.map((p) => ({ ...p })),
    organization: c.organization ?? "",
    jobTitle: c.jobTitle ?? "",
    notes: c.notes ?? "",
  };
}

/** Builds the JMAP draft from the form, dropping empty rows and blank fields. */
export function toDraft(f: FormState): ContactDraft {
  const clean = (fields: ContactField[]) =>
    fields
      .map((x) => ({ kind: x.kind, value: x.value.trim() }))
      .filter((x) => x.value !== "");
  const trimOrNull = (s: string) => (s.trim() === "" ? null : s.trim());
  const draft: ContactDraft = {
    firstName: trimOrNull(f.firstName),
    lastName: trimOrNull(f.lastName),
    emails: clean(f.emails),
    phones: clean(f.phones),
    organization: trimOrNull(f.organization),
    jobTitle: trimOrNull(f.jobTitle),
    notes: trimOrNull(f.notes),
  };
  // Only send `name` when the user typed one; otherwise let the server
  // derive it (from N or the first email) — omitting the key entirely is
  // required under exactOptionalPropertyTypes.
  const name = f.displayName.trim();
  if (name !== "") draft.name = name;
  return draft;
}

export function ContactsModal({ onClose }: ContactsModalProps) {
  const client = useJmapClient();
  const { confirm } = useDialogs();
  const [contacts, setContacts] = useState<Contact[] | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [query, setQuery] = useState("");
  // The selected contact id, or "new" while composing a fresh one, or null.
  const [selected, setSelected] = useState<string | "new" | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  const load = useCallback(() => {
    setLoadError(false);
    client
      .contacts()
      .then(setContacts)
      .catch(() => setLoadError(true));
  }, [client]);

  useEffect(load, [load]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const filtered = useMemo(() => {
    const list = contacts ?? [];
    const q = query.trim().toLowerCase();
    if (q === "") return list;
    return list.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        c.emails.some((e) => e.value.toLowerCase().includes(q)) ||
        (c.organization ?? "").toLowerCase().includes(q),
    );
  }, [contacts, query]);

  function openNew() {
    setSelected("new");
    setForm(EMPTY);
    setFormError(null);
  }

  function openContact(c: Contact) {
    setSelected(c.id);
    setForm(toForm(c));
    setFormError(null);
  }

  async function save() {
    const draft = toDraft(form);
    // The server also enforces this, but a local check gives an instant,
    // localized message instead of a round-trip.
    if (
      (draft.name === undefined || draft.name === null) &&
      (draft.emails === undefined || draft.emails.length === 0) &&
      (draft.firstName ?? null) === null &&
      (draft.lastName ?? null) === null
    ) {
      setFormError(strings.contactNeedsName);
      return;
    }
    setBusy(true);
    setFormError(null);
    try {
      if (selected === "new") {
        const id = await client.createContact(draft);
        const fresh = await client.contacts();
        setContacts(fresh);
        setSelected(id);
        const created = fresh.find((c) => c.id === id);
        if (created) setForm(toForm(created));
      } else if (selected !== null) {
        await client.updateContact(selected, draft);
        setContacts(await client.contacts());
      }
    } catch {
      setFormError(strings.contactSaveError);
    } finally {
      setBusy(false);
    }
  }

  async function onImportFile(file: File) {
    setBusy(true);
    setNotice(null);
    try {
      const vcf = await file.text();
      const { imported, skipped } = await client.importContacts(vcf);
      setContacts(await client.contacts());
      setNotice(strings.contactsImported(imported, skipped));
    } catch {
      setNotice(strings.contactsImportError);
    } finally {
      setBusy(false);
      if (fileInput.current) fileInput.current.value = "";
    }
  }

  async function onExport() {
    if ((contacts?.length ?? 0) === 0) {
      setNotice(strings.contactsExportEmpty);
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      const vcf = await client.exportContacts();
      const url = URL.createObjectURL(new Blob([vcf], { type: "text/vcard" }));
      const a = document.createElement("a");
      a.href = url;
      a.download = "contacts.vcf";
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      setNotice(strings.contactsExportError);
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (selected === null || selected === "new") return;
    const contact = (contacts ?? []).find((c) => c.id === selected);
    const label = contact?.name ?? "";
    if (!(await confirm({ message: strings.contactDeleteConfirm(label), danger: true }))) return;
    setBusy(true);
    try {
      await client.deleteContact(selected);
      setContacts(await client.contacts());
      setSelected(null);
    } catch {
      setFormError(strings.contactDeleteError);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div
        className={styles.modal}
        role="dialog"
        aria-modal="true"
        aria-label={strings.contactsTitle}
        onClick={(e) => e.stopPropagation()}
      >
        <div className={styles.head}>
          <h2 className={styles.title}>{strings.contactsTitle}</h2>
          <div className={styles.headActions}>
            <input
              ref={fileInput}
              type="file"
              accept=".vcf,text/vcard"
              hidden
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) void onImportFile(file);
              }}
            />
            <button
              type="button"
              className={styles.headBtn}
              onClick={() => fileInput.current?.click()}
              disabled={busy}
            >
              <Upload size={15} />
              {busy ? strings.contactsImporting : strings.contactsImport}
            </button>
            <button
              type="button"
              className={styles.headBtn}
              onClick={onExport}
              disabled={busy}
            >
              <Download size={15} />
              {strings.contactsExport}
            </button>
            <button
              type="button"
              className={styles.close}
              onClick={onClose}
              aria-label={strings.contactCancel}
            >
              <X size={18} />
            </button>
          </div>
        </div>
        {notice !== null && <p className={styles.notice}>{notice}</p>}

        <div className={styles.body}>
          <div className={styles.list}>
            <div className={styles.search}>
              <Search size={15} aria-hidden />
              <input
                type="search"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={strings.contactsSearchPlaceholder}
                aria-label={strings.contactsSearchPlaceholder}
              />
            </div>
            <button type="button" className={styles.newRow} onClick={openNew}>
              <UserPlus size={16} />
              <span>{strings.contactsNew}</span>
            </button>

            {contacts === null && !loadError && (
              <div className={styles.centered}>
                <Spinner size={20} />
              </div>
            )}
            {loadError && (
              <div className={styles.centered}>
                <p>{strings.contactsLoadError}</p>
                <Button variant="secondary" size="sm" onClick={load}>
                  {strings.mailRetry}
                </Button>
              </div>
            )}
            {contacts !== null && !loadError && filtered.length === 0 && (
              <p className={styles.empty}>
                {query.trim() === "" ? strings.contactsEmpty : strings.contactsSearchEmpty}
              </p>
            )}
            {filtered.map((c) => (
              <button
                type="button"
                key={c.id}
                className={cx(styles.item, selected === c.id && styles.itemActive)}
                onClick={() => openContact(c)}
              >
                <span className={styles.itemName}>{c.name}</span>
                <span className={styles.itemSub}>
                  {c.emails[0]?.value ?? c.organization ?? strings.contactNoEmail}
                </span>
              </button>
            ))}
          </div>

          <div className={styles.detail}>
            {selected === null ? (
              <div className={styles.placeholder}>
                <Mail size={28} aria-hidden />
                <p>{strings.contactsEmpty}</p>
              </div>
            ) : (
              <ContactForm
                form={form}
                setForm={setForm}
                isNew={selected === "new"}
                busy={busy}
                error={formError}
                onSave={save}
                onDelete={remove}
                onCancel={() => setSelected(null)}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

interface ContactFormProps {
  form: FormState;
  setForm: (f: FormState) => void;
  isNew: boolean;
  busy: boolean;
  error: string | null;
  onSave: () => void;
  onDelete: () => void;
  onCancel: () => void;
}

function ContactForm({
  form,
  setForm,
  isNew,
  busy,
  error,
  onSave,
  onDelete,
  onCancel,
}: ContactFormProps) {
  const patch = (p: Partial<FormState>) => setForm({ ...form, ...p });

  const setField = (key: "emails" | "phones", i: number, next: Partial<ContactField>) => {
    const arr = form[key].map((f, idx) => (idx === i ? { ...f, ...next } : f));
    patch({ [key]: arr } as Partial<FormState>);
  };
  const addField = (key: "emails" | "phones") =>
    patch({ [key]: [...form[key], { kind: null, value: "" }] } as Partial<FormState>);
  const removeField = (key: "emails" | "phones", i: number) =>
    patch({ [key]: form[key].filter((_, idx) => idx !== i) } as Partial<FormState>);

  return (
    <form
      className={styles.form}
      onSubmit={(e) => {
        e.preventDefault();
        onSave();
      }}
    >
      <div className={styles.names}>
        <label className={styles.field}>
          <span>{strings.contactFirstName}</span>
          <input
            value={form.firstName}
            onChange={(e) => patch({ firstName: e.target.value })}
            autoFocus={isNew}
          />
        </label>
        <label className={styles.field}>
          <span>{strings.contactLastName}</span>
          <input value={form.lastName} onChange={(e) => patch({ lastName: e.target.value })} />
        </label>
      </div>

      <label className={styles.field}>
        <span>{strings.contactDisplayName}</span>
        <input
          value={form.displayName}
          onChange={(e) => patch({ displayName: e.target.value })}
        />
      </label>

      <FieldList
        legend={strings.contactEmail}
        icon={<Mail size={15} aria-hidden />}
        addLabel={strings.contactAddEmail}
        type="email"
        fields={form.emails}
        onChange={(i, next) => setField("emails", i, next)}
        onAdd={() => addField("emails")}
        onRemove={(i) => removeField("emails", i)}
      />
      <FieldList
        legend={strings.contactPhone}
        icon={<Phone size={15} aria-hidden />}
        addLabel={strings.contactAddPhone}
        type="tel"
        fields={form.phones}
        onChange={(i, next) => setField("phones", i, next)}
        onAdd={() => addField("phones")}
        onRemove={(i) => removeField("phones", i)}
      />

      <div className={styles.names}>
        <label className={styles.field}>
          <span>{strings.contactOrganization}</span>
          <input
            value={form.organization}
            onChange={(e) => patch({ organization: e.target.value })}
          />
        </label>
        <label className={styles.field}>
          <span>{strings.contactJobTitle}</span>
          <input value={form.jobTitle} onChange={(e) => patch({ jobTitle: e.target.value })} />
        </label>
      </div>

      <label className={styles.field}>
        <span>{strings.contactNotes}</span>
        <textarea
          rows={3}
          value={form.notes}
          onChange={(e) => patch({ notes: e.target.value })}
        />
      </label>

      {error !== null && <p className={styles.error}>{error}</p>}

      <div className={styles.actions}>
        {!isNew && (
          <Button type="button" variant="danger" size="sm" onClick={onDelete} disabled={busy}>
            <Trash2 size={15} />
            {strings.contactDelete}
          </Button>
        )}
        <div className={styles.actionsRight}>
          <Button type="button" variant="ghost" size="sm" onClick={onCancel} disabled={busy}>
            {strings.contactCancel}
          </Button>
          <Button type="submit" size="sm" disabled={busy}>
            {strings.contactSave}
          </Button>
        </div>
      </div>
    </form>
  );
}

interface FieldListProps {
  legend: string;
  icon: React.ReactNode;
  addLabel: string;
  type: "email" | "tel";
  fields: ContactField[];
  onChange: (i: number, next: Partial<ContactField>) => void;
  onAdd: () => void;
  onRemove: (i: number) => void;
}

function FieldList({ legend, icon, addLabel, type, fields, onChange, onAdd, onRemove }: FieldListProps) {
  // Read inside render so the labels follow the active locale (a module-level
  // const would capture the language at import time and go stale on a switch).
  const kindOptions: ReadonlyArray<{ value: string; label: string }> = [
    { value: "work", label: strings.contactKindWork },
    { value: "home", label: strings.contactKindHome },
    { value: "mobile", label: strings.contactKindMobile },
    { value: "other", label: strings.contactKindOther },
  ];
  return (
    <fieldset className={styles.fieldset}>
      <legend>{legend}</legend>
      {fields.map((field, i) => (
        <div className={styles.multiRow} key={i}>
          <span className={styles.rowIcon}>{icon}</span>
          <input
            className={styles.rowValue}
            type={type}
            value={field.value}
            onChange={(e) => onChange(i, { value: e.target.value })}
            placeholder={legend}
            aria-label={legend}
          />
          <select
            className={styles.rowKind}
            value={field.kind ?? ""}
            onChange={(e) => onChange(i, { kind: e.target.value === "" ? null : e.target.value })}
            aria-label={strings.contactKindOther}
          >
            <option value="">—</option>
            {kindOptions.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
          <button
            type="button"
            className={styles.rowRemove}
            onClick={() => onRemove(i)}
            aria-label={strings.contactRemoveField}
          >
            <X size={14} />
          </button>
        </div>
      ))}
      <button type="button" className={styles.addRow} onClick={onAdd}>
        <Plus size={14} />
        {addLabel}
      </button>
    </fieldset>
  );
}
