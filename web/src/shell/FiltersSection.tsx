// Server-side mail filters (rules) editor, embedded in Settings. Each rule is
// a set of conditions (field · match · value) and actions (file into a folder,
// mark read, star, delete). The server compiles the rules into a Sieve script
// that runs at delivery. Rules persist immediately on save/delete/toggle.
import { useEffect, useMemo, useState } from "react";
import { Plus, Trash2, X } from "lucide-react";

import { strings } from "../i18n";
import { Button, Spinner } from "../ds";
import { useJmapClient } from "../jmap";
import type {
  FilterAction,
  FilterCondition,
  FilterField,
  FilterMatch,
  FilterOp,
  MailFilterRule,
  Mailbox,
} from "../jmap";
import admin from "../admin/admin.module.css";
import styles from "./FiltersSection.module.css";

/** A blank draft rule with one empty condition. */
function blankRule(): MailFilterRule {
  return {
    id: crypto.randomUUID(),
    name: "",
    match: "all",
    conditions: [{ field: "from", op: "contains", value: "" }],
    actions: [],
    enabled: true,
  };
}

const FIELD_LABELS: Record<FilterField, string> = {
  from: strings.filterFieldFrom,
  to: strings.filterFieldTo,
  cc: strings.filterFieldCc,
  subject: strings.filterFieldSubject,
};

/** A one-line human summary of a rule for the list. */
function summarize(rule: MailFilterRule): string {
  const conds = rule.conditions
    .map(
      (c) =>
        `${FIELD_LABELS[c.field]} ${c.op === "is" ? "=" : "∋"} "${c.value}"`,
    )
    .join(rule.match === "all" ? " · " : ` ${strings.filterOr} `);
  const acts = rule.actions
    .map((a) => {
      switch (a.type) {
        case "fileInto":
          return `${strings.filterActionFileInto} ${a.mailbox}`;
        case "markRead":
          return strings.filterActionMarkRead;
        case "star":
          return strings.filterActionStar;
        case "delete":
          return strings.filterActionDelete;
      }
    })
    .join(", ");
  return `${conds} → ${acts}`;
}

export function FiltersSection() {
  const client = useJmapClient();
  const [rules, setRules] = useState<MailFilterRule[]>([]);
  const [mailboxes, setMailboxes] = useState<Mailbox[]>([]);
  const [draft, setDraft] = useState<MailFilterRule | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    Promise.all([client.filters(), client.mailboxes()])
      .then(([r, m]) => {
        if (!live) return;
        setRules(r);
        setMailboxes(m);
        setLoaded(true);
      })
      .catch(() => {
        if (live) setError(strings.filtersLoadError);
      });
    return () => {
      live = false;
    };
  }, [client]);

  // Folders offered as "file into" targets (skip virtual/managed roles).
  const folderTargets = useMemo(
    () =>
      mailboxes.filter((m) => m.role !== "snoozed" && m.role !== "scheduled"),
    [mailboxes],
  );

  async function persist(next: MailFilterRule[]) {
    setBusy(true);
    setError(null);
    try {
      const saved = await client.saveFilters(next);
      setRules(saved);
      setDraft(null);
    } catch {
      setError(strings.filtersSaveError);
    } finally {
      setBusy(false);
    }
  }

  function saveDraft() {
    if (draft === null) return;
    const conditions = draft.conditions.filter(
      (c) => c.value.trim().length > 0,
    );
    if (conditions.length === 0) {
      setError(strings.filterNeedsCondition);
      return;
    }
    if (draft.actions.length === 0) {
      setError(strings.filterNeedsAction);
      return;
    }
    const clean: MailFilterRule = { ...draft, conditions };
    const idx = rules.findIndex((r) => r.id === clean.id);
    const next =
      idx >= 0
        ? rules.map((r) => (r.id === clean.id ? clean : r))
        : [...rules, clean];
    void persist(next);
  }

  function toggle(rule: MailFilterRule) {
    void persist(
      rules.map((r) => (r.id === rule.id ? { ...r, enabled: !r.enabled } : r)),
    );
  }

  function remove(rule: MailFilterRule) {
    void persist(rules.filter((r) => r.id !== rule.id));
  }

  if (!loaded && error === null) {
    return (
      <div className={styles.loading}>
        <Spinner size={18} />
      </div>
    );
  }

  return (
    <div className={styles.section}>
      {rules.length > 0 && (
        <ul className={styles.list}>
          {rules.map((rule) => (
            <li key={rule.id} className={styles.rule}>
              <label className={styles.enable}>
                <input
                  type="checkbox"
                  checked={rule.enabled}
                  onChange={() => toggle(rule)}
                />
              </label>
              <button
                type="button"
                className={styles.ruleBody}
                onClick={() => setDraft(structuredClone(rule))}
              >
                {rule.name.trim().length > 0 && (
                  <span className={styles.ruleName}>{rule.name}</span>
                )}
                <span className={styles.ruleSummary}>{summarize(rule)}</span>
              </button>
              <button
                type="button"
                className={styles.ruleDelete}
                onClick={() => remove(rule)}
                aria-label={strings.filterDelete}
              >
                <Trash2 size={15} />
              </button>
            </li>
          ))}
        </ul>
      )}

      {draft === null ? (
        <button
          type="button"
          className={styles.addRule}
          onClick={() => setDraft(blankRule())}
          disabled={busy}
        >
          <Plus size={15} />
          <span>{strings.filterAddRule}</span>
        </button>
      ) : (
        <RuleEditor
          draft={draft}
          folders={folderTargets}
          busy={busy}
          onChange={setDraft}
          onSave={saveDraft}
          onCancel={() => {
            setDraft(null);
            setError(null);
          }}
        />
      )}

      {error !== null && (
        <p className={admin.error} role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

interface RuleEditorProps {
  draft: MailFilterRule;
  folders: Mailbox[];
  busy: boolean;
  onChange: (next: MailFilterRule) => void;
  onSave: () => void;
  onCancel: () => void;
}

function RuleEditor({
  draft,
  folders,
  busy,
  onChange,
  onSave,
  onCancel,
}: RuleEditorProps) {
  function setCondition(i: number, patch: Partial<FilterCondition>) {
    onChange({
      ...draft,
      conditions: draft.conditions.map((c, j) =>
        j === i ? { ...c, ...patch } : c,
      ),
    });
  }
  function addCondition() {
    onChange({
      ...draft,
      conditions: [
        ...draft.conditions,
        { field: "from", op: "contains", value: "" },
      ],
    });
  }
  function removeCondition(i: number) {
    onChange({
      ...draft,
      conditions: draft.conditions.filter((_, j) => j !== i),
    });
  }

  // Actions are edited as a small set of toggles; delete is exclusive.
  const fileInto = draft.actions.find((a) => a.type === "fileInto");
  const has = (t: FilterAction["type"]) =>
    draft.actions.some((a) => a.type === t);
  function setActions(next: FilterAction[]) {
    onChange({ ...draft, actions: next });
  }
  function toggleAction(action: FilterAction) {
    if (action.type === "delete") {
      // Delete is exclusive — it replaces every other action.
      setActions(has("delete") ? [] : [{ type: "delete" }]);
      return;
    }
    const without = draft.actions.filter(
      (a) => a.type !== action.type && a.type !== "delete",
    );
    setActions(has(action.type) ? without : [...without, action]);
  }
  function setFolder(mailbox: string) {
    const without = draft.actions.filter((a) => a.type !== "fileInto");
    setActions([{ type: "fileInto", mailbox }, ...without]);
  }

  const firstFolder = folders[0]?.name ?? "";

  return (
    <div className={styles.editor}>
      <input
        className={admin.input}
        value={draft.name}
        onChange={(e) => onChange({ ...draft, name: e.target.value })}
        placeholder={strings.filterNamePlaceholder}
      />

      <div className={styles.subhead}>
        <span>{strings.filterWhen}</span>
        {draft.conditions.length > 1 && (
          <span className={styles.matchToggle}>
            {(["all", "any"] as FilterMatch[]).map((m) => (
              <button
                key={m}
                type="button"
                className={draft.match === m ? styles.matchOn : styles.matchOff}
                onClick={() => onChange({ ...draft, match: m })}
              >
                {m === "all" ? strings.filterMatchAll : strings.filterMatchAny}
              </button>
            ))}
          </span>
        )}
      </div>

      {draft.conditions.map((c, i) => (
        <div key={i} className={styles.condRow}>
          <select
            className={styles.select}
            value={c.field}
            onChange={(e) =>
              setCondition(i, { field: e.target.value as FilterField })
            }
          >
            <option value="from">{strings.filterFieldFrom}</option>
            <option value="to">{strings.filterFieldTo}</option>
            <option value="cc">{strings.filterFieldCc}</option>
            <option value="subject">{strings.filterFieldSubject}</option>
          </select>
          <select
            className={styles.select}
            value={c.op}
            onChange={(e) =>
              setCondition(i, { op: e.target.value as FilterOp })
            }
          >
            <option value="contains">{strings.filterOpContains}</option>
            <option value="is">{strings.filterOpIs}</option>
          </select>
          <input
            className={admin.input}
            value={c.value}
            onChange={(e) => setCondition(i, { value: e.target.value })}
            placeholder={strings.filterValuePlaceholder}
          />
          {draft.conditions.length > 1 && (
            <button
              type="button"
              className={styles.condRemove}
              onClick={() => removeCondition(i)}
              aria-label={strings.filterRemoveCondition}
            >
              <X size={14} />
            </button>
          )}
        </div>
      ))}
      <button type="button" className={styles.addCond} onClick={addCondition}>
        <Plus size={14} />
        <span>{strings.filterAddCondition}</span>
      </button>

      <div className={styles.subhead}>
        <span>{strings.filterDo}</span>
      </div>
      <div className={styles.actions}>
        <label className={styles.action}>
          <input
            type="checkbox"
            checked={fileInto !== undefined}
            disabled={has("delete")}
            onChange={() =>
              toggleAction({
                type: "fileInto",
                mailbox: fileInto?.mailbox ?? firstFolder,
              })
            }
          />
          <span>{strings.filterActionFileInto}</span>
          {fileInto !== undefined && (
            <select
              className={styles.select}
              value={fileInto.mailbox}
              onChange={(e) => setFolder(e.target.value)}
            >
              {folders.map((m) => (
                <option key={m.id} value={m.name}>
                  {m.name}
                </option>
              ))}
            </select>
          )}
        </label>
        <label className={styles.action}>
          <input
            type="checkbox"
            checked={has("markRead")}
            disabled={has("delete")}
            onChange={() => toggleAction({ type: "markRead" })}
          />
          <span>{strings.filterActionMarkRead}</span>
        </label>
        <label className={styles.action}>
          <input
            type="checkbox"
            checked={has("star")}
            disabled={has("delete")}
            onChange={() => toggleAction({ type: "star" })}
          />
          <span>{strings.filterActionStar}</span>
        </label>
        <label className={styles.action}>
          <input
            type="checkbox"
            checked={has("delete")}
            onChange={() => toggleAction({ type: "delete" })}
          />
          <span>{strings.filterActionDelete}</span>
        </label>
      </div>

      <div className={styles.editorFoot}>
        <button type="button" className={styles.cancel} onClick={onCancel}>
          {strings.filterCancel}
        </button>
        <Button onClick={onSave} disabled={busy}>
          {busy ? <Spinner size={15} /> : strings.filterSaveRule}
        </Button>
      </div>
    </div>
  );
}
