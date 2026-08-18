// The address book, opened from the account menu. Two panes: a searchable
// contact list on the left, and the selected contact's editable detail on
// the right. Create, edit, and delete are wired straight to the JMAP
// Contact API (Contact/get + Contact/set). Kept as a modal (like Settings)
// so it needs no route — no Caddy prefix to collide with the API.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Download,
  Mail,
  Phone,
  Plus,
  Search,
  Trash2,
  Upload,
  UserPlus,
  X,
} from "lucide-react";

import { strings } from "../i18n";
import {
  Button,
  Field,
  IconButton,
  Input,
  Modal,
  Select,
  Spinner,
  cx,
  useDialogs,
} from "../ds";
import { useJmapClient } from "../jmap";
import type { Contact, ContactDraft, ContactField } from "../jmap";

// The layout this screen keeps for itself (D2.06). Everything that was a
// primitive — the panel, the scrim, the fields, the kind picker, the buttons —
// is `ds/`; what is left below is the two-pane arrangement and the list row,
// which are this screen's own shape rather than anything the design system
// should own.
//
// Written as whole strings rather than layered, for the reason `ds/Modal`
// gives: two utilities setting one property have no defined winner in
// Tailwind's output order, so a state that replaces a value replaces the whole
// string (see `ITEM` / `ITEM_ON`).

/** The import/export report, above the two panes and across both. */
const NOTICE =
  "shrink-0 border-b border-subtle bg-raised px-5 py-2 text-sm text-secondary";

/** The two panes. A fixed list column, and the detail taking the rest; on a
 *  phone the list stacks above the detail, which is what the stylesheet's one
 *  media query did. */
const PANES = "grid min-h-0 flex-1 grid-cols-[300px_1fr] max-sm:grid-cols-1";
const LIST =
  "flex min-h-0 flex-col gap-0.5 overflow-y-auto border-r border-subtle p-3 max-sm:border-b max-sm:border-r-0";
const DETAIL = "min-h-0 overflow-y-auto p-5";

/** The search box: `ds/Input` with the magnifier laid over its trailing end,
 *  which is how billing's search reads too. The icon is decoration — the input
 *  carries the name — so it is out of the pointer's way and hidden from
 *  assistive technology. */
const SEARCH_WRAP =
  "relative mb-2 shrink-0 [&>svg]:pointer-events-none [&>svg]:absolute [&>svg]:right-3 [&>svg]:top-1/2 [&>svg]:-translate-y-1/2 [&>svg]:text-tertiary";

/** "New contact" and "Add email" — an accent line that starts an item. Not a
 *  `ds/Button`: none of the four variants is a borderless accent row, and
 *  widening `Button` for a list affordance would blur the thing it names. */
const ADD_ROW =
  "flex shrink-0 items-center gap-2 rounded-md px-3 py-2 text-left text-sm font-medium text-accent hover:bg-raised";

/** A contact in the list. The selected row replaces the resting background
 *  rather than layering over it, so the hover cannot win back over it. */
const ITEM_BASE =
  "flex w-full shrink-0 flex-col gap-px rounded-md px-3 py-2 text-left";
const ITEM = "hover:bg-raised";
const ITEM_ON = "bg-selected";
const ITEM_NAME = "truncate text-sm font-medium text-primary";
const ITEM_SUB = "truncate text-xs text-tertiary";

/** The list's own three placeholders — loading, failed, empty. */
const CENTERED =
  "flex flex-col items-center gap-3 px-4 py-6 text-center text-sm text-tertiary";

/** The detail pane before a contact is chosen. */
const PLACEHOLDER =
  "flex h-full flex-col items-center justify-center gap-3 text-center text-tertiary";

const FORM = "flex flex-col gap-4";
/** First/last name and organization/job title sit two to a row, and stack on a
 *  phone with the panes. */
const PAIR = "grid grid-cols-2 gap-3 max-sm:grid-cols-1";
/** A multi-value group. `ds/Field` is one label over one control; this is one
 *  label over a list of rows, so the `<fieldset>`/`<legend>` stays. */
const FIELDSET = "flex flex-col gap-2 border-0 p-0 m-0";
const LEGEND = "mb-1 p-0 text-sm font-medium text-primary";
const MULTI_ROW = "flex items-center gap-2";
const ROW_ICON = "inline-flex shrink-0 text-tertiary";
const FORM_ERROR = "text-sm leading-snug text-danger";
const ACTIONS =
  "flex items-center justify-between gap-3 border-t border-subtle pt-3";
const ACTIONS_RIGHT = "ml-auto flex gap-2";

/** The notes box. There is no multi-line control in `ds/` yet (recorded for
 *  D3.01), so this is `ds/Input`'s box written out for a `<textarea>` — the
 *  same border, radius, height and focus ring, so the two do not read as
 *  different kinds of control in one form. */
const TEXTAREA =
  "w-full resize-y rounded-md border border-default bg-surface px-3 py-2 " +
  "font-[inherit] text-base text-primary placeholder:text-tertiary " +
  "transition-colors duration-[var(--duration-fast)] ease-standard " +
  "focus-visible:border-strong focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent";

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
    emails:
      c.emails.length > 0
        ? c.emails.map((e) => ({ ...e }))
        : [{ kind: null, value: "" }],
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
    if (
      !(await confirm({
        message: strings.contactDeleteConfirm(label),
        danger: true,
      }))
    )
      return;
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
    <Modal
      title={strings.contactsTitle}
      onClose={onClose}
      wide
      tall="page"
      actions={
        <>
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
          <Button
            variant="ghost"
            size="sm"
            icon={<Upload size={15} />}
            onClick={() => fileInput.current?.click()}
            disabled={busy}
          >
            {busy ? strings.contactsImporting : strings.contactsImport}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            icon={<Download size={15} />}
            onClick={onExport}
            disabled={busy}
          >
            {strings.contactsExport}
          </Button>
          <IconButton
            label={strings.userClose}
            icon={<X />}
            onClick={onClose}
          />
        </>
      }
    >
      {notice !== null && <p className={NOTICE}>{notice}</p>}

      <div className={PANES}>
        <div className={LIST}>
          <div className={SEARCH_WRAP}>
            <Input
              type="search"
              className="pr-9"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={strings.contactsSearchPlaceholder}
              aria-label={strings.contactsSearchPlaceholder}
            />
            <Search size={15} aria-hidden />
          </div>
          <button type="button" className={ADD_ROW} onClick={openNew}>
            <UserPlus size={16} />
            <span>{strings.contactsNew}</span>
          </button>

          {contacts === null && !loadError && (
            <div className={CENTERED}>
              <Spinner size={20} />
            </div>
          )}
          {loadError && (
            <div className={CENTERED}>
              <p>{strings.contactsLoadError}</p>
              <Button variant="secondary" size="sm" onClick={load}>
                {strings.mailRetry}
              </Button>
            </div>
          )}
          {contacts !== null && !loadError && filtered.length === 0 && (
            <p className={CENTERED}>
              {query.trim() === ""
                ? strings.contactsEmpty
                : strings.contactsSearchEmpty}
            </p>
          )}
          {filtered.map((c) => (
            <button
              type="button"
              key={c.id}
              className={cx(ITEM_BASE, selected === c.id ? ITEM_ON : ITEM)}
              aria-current={selected === c.id}
              onClick={() => openContact(c)}
            >
              <span className={ITEM_NAME}>{c.name}</span>
              <span className={ITEM_SUB}>
                {c.emails[0]?.value ?? c.organization ?? strings.contactNoEmail}
              </span>
            </button>
          ))}
        </div>

        <div className={DETAIL}>
          {selected === null ? (
            <div className={PLACEHOLDER}>
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
    </Modal>
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

  const setField = (
    key: "emails" | "phones",
    i: number,
    next: Partial<ContactField>,
  ) => {
    const arr = form[key].map((f, idx) => (idx === i ? { ...f, ...next } : f));
    patch({ [key]: arr } as Partial<FormState>);
  };
  const addField = (key: "emails" | "phones") =>
    patch({
      [key]: [...form[key], { kind: null, value: "" }],
    } as Partial<FormState>);
  const removeField = (key: "emails" | "phones", i: number) =>
    patch({
      [key]: form[key].filter((_, idx) => idx !== i),
    } as Partial<FormState>);

  return (
    <form
      className={FORM}
      onSubmit={(e) => {
        e.preventDefault();
        onSave();
      }}
    >
      <div className={PAIR}>
        <Field label={strings.contactFirstName}>
          {(control) => (
            <Input
              {...control}
              value={form.firstName}
              onChange={(e) => patch({ firstName: e.target.value })}
              autoFocus={isNew}
            />
          )}
        </Field>
        <Field label={strings.contactLastName}>
          {(control) => (
            <Input
              {...control}
              value={form.lastName}
              onChange={(e) => patch({ lastName: e.target.value })}
            />
          )}
        </Field>
      </div>

      <Field label={strings.contactDisplayName}>
        {(control) => (
          <Input
            {...control}
            value={form.displayName}
            onChange={(e) => patch({ displayName: e.target.value })}
          />
        )}
      </Field>

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

      <div className={PAIR}>
        <Field label={strings.contactOrganization}>
          {(control) => (
            <Input
              {...control}
              value={form.organization}
              onChange={(e) => patch({ organization: e.target.value })}
            />
          )}
        </Field>
        <Field label={strings.contactJobTitle}>
          {(control) => (
            <Input
              {...control}
              value={form.jobTitle}
              onChange={(e) => patch({ jobTitle: e.target.value })}
            />
          )}
        </Field>
      </div>

      <Field label={strings.contactNotes}>
        {(control) => (
          <textarea
            id={control.id}
            aria-describedby={control["aria-describedby"]}
            className={TEXTAREA}
            rows={3}
            value={form.notes}
            onChange={(e) => patch({ notes: e.target.value })}
          />
        )}
      </Field>

      {error !== null && (
        <p className={FORM_ERROR} role="alert">
          {error}
        </p>
      )}

      <div className={ACTIONS}>
        {!isNew && (
          <Button
            type="button"
            variant="danger"
            size="sm"
            icon={<Trash2 size={15} />}
            onClick={onDelete}
            disabled={busy}
          >
            {strings.contactDelete}
          </Button>
        )}
        <div className={ACTIONS_RIGHT}>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onCancel}
            disabled={busy}
          >
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

function FieldList({
  legend,
  icon,
  addLabel,
  type,
  fields,
  onChange,
  onAdd,
  onRemove,
}: FieldListProps) {
  // Read inside render so the labels follow the active locale (a module-level
  // const would capture the language at import time and go stale on a switch).
  const kindOptions: ReadonlyArray<{ value: string; label: string }> = [
    { value: "work", label: strings.contactKindWork },
    { value: "home", label: strings.contactKindHome },
    { value: "mobile", label: strings.contactKindMobile },
    { value: "other", label: strings.contactKindOther },
  ];
  return (
    <fieldset className={FIELDSET}>
      <legend className={LEGEND}>{legend}</legend>
      {fields.map((field, i) => {
        // Every row of a group used to be announced identically — four
        // "Remove" commands and four combo boxes reading out their own current
        // value as their name. The row's value is what tells them apart, and a
        // row with nothing in it yet falls back to the group's name rather
        // than to nothing.
        const rowName = field.value.trim() === "" ? legend : field.value.trim();
        return (
          <div className={MULTI_ROW} key={i}>
            <span className={ROW_ICON} aria-hidden="true">
              {icon}
            </span>
            <Input
              type={type}
              value={field.value}
              onChange={(e) => onChange(i, { value: e.target.value })}
              placeholder={legend}
              aria-label={legend}
            />
            <Select
              className="w-24 shrink-0"
              value={field.kind ?? ""}
              onChange={(e) =>
                onChange(i, {
                  kind: e.target.value === "" ? null : e.target.value,
                })
              }
              aria-label={strings.contactKindLabel(rowName)}
            >
              <option value="">—</option>
              {kindOptions.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
            <IconButton
              label={strings.contactRemoveFieldNamed(rowName)}
              icon={<X />}
              onClick={() => onRemove(i)}
            />
          </div>
        );
      })}
      <button type="button" className={ADD_ROW} onClick={onAdd}>
        <Plus size={14} />
        {addLabel}
      </button>
    </fieldset>
  );
}
