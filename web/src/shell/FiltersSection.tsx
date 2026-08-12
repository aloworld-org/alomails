// Server-side mail filters (rules) editor, embedded in Settings. Each rule is
// a set of conditions (field · match · value) and actions (file into a folder,
// mark read, star, delete). The server compiles the rules into a Sieve script
// that runs at delivery. Rules persist immediately on save/delete/toggle.
import { useEffect, useMemo, useState } from "react";
import { Plus, Trash2, X } from "lucide-react";

import { strings } from "../i18n";
import { Button, Checkbox, IconButton, Input, Select, Spinner } from "../ds";
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

/** What to call a rule when a control has to say which rule it belongs to. A
 *  rule's name is optional — most people never type one — so the fallback is
 *  the summary the row already shows, never an empty string. */
function ruleTitle(rule: MailFilterRule): string {
  const named = rule.name.trim();
  return named.length > 0 ? named : summarize(rule);
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
              {/* The box says which rule it switches. Before: a bare box in a
                  wrapping label with no text in it — a column announced as
                  "checkbox, checked" as many times as there are rules. An
                  unnamed rule is named by what it does, which is the same
                  sentence the row already draws. */}
              <Checkbox
                checked={rule.enabled}
                onChange={() => toggle(rule)}
                label={strings.filterRuleEnabled(ruleTitle(rule))}
                hideLabel
                className={styles.enable}
              />
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
              <IconButton
                label={strings.filterDelete}
                icon={<Trash2 />}
                onClick={() => remove(rule)}
                className={styles.ruleDelete}
              />
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
        <p className={styles.error} role="alert">
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
      <Input
        value={draft.name}
        onChange={(e) => onChange({ ...draft, name: e.target.value })}
        placeholder={strings.filterNamePlaceholder}
        aria-label={strings.filterNamePlaceholder}
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

      {/* Each condition is three controls, and every one of them used to be
          anonymous: two selects with no label at all — the pair `ds/Select` was
          written about, announced as "combo box, From" — and a value box with
          only a placeholder. They are numbered because a rule can hold several
          and "field" three times over says nothing about which row. */}
      {draft.conditions.map((c, i) => (
        <div key={i} className={styles.condRow}>
          <Select
            value={c.field}
            aria-label={strings.filterConditionField(i + 1)}
            onChange={(e) =>
              setCondition(i, { field: e.target.value as FilterField })
            }
          >
            <option value="from">{strings.filterFieldFrom}</option>
            <option value="to">{strings.filterFieldTo}</option>
            <option value="cc">{strings.filterFieldCc}</option>
            <option value="subject">{strings.filterFieldSubject}</option>
          </Select>
          <Select
            value={c.op}
            aria-label={strings.filterConditionOp(i + 1)}
            onChange={(e) =>
              setCondition(i, { op: e.target.value as FilterOp })
            }
          >
            <option value="contains">{strings.filterOpContains}</option>
            <option value="is">{strings.filterOpIs}</option>
          </Select>
          <Input
            value={c.value}
            aria-label={strings.filterConditionValue(i + 1)}
            onChange={(e) => setCondition(i, { value: e.target.value })}
            placeholder={strings.filterValuePlaceholder}
          />
          {draft.conditions.length > 1 && (
            <IconButton
              label={strings.filterRemoveConditionAt(i + 1)}
              icon={<X />}
              onClick={() => removeCondition(i)}
              className={styles.condRemove}
            />
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
        {/* The folder picker is beside the box, not inside its label: a label
            binds to the first control it contains, so wrapping both left the
            select unnamed and made clicking its words tick the checkbox. */}
        <div className={styles.action}>
          <Checkbox
            checked={fileInto !== undefined}
            disabled={has("delete")}
            onChange={() =>
              toggleAction({
                type: "fileInto",
                mailbox: fileInto?.mailbox ?? firstFolder,
              })
            }
            label={strings.filterActionFileInto}
          />
          {fileInto !== undefined && (
            <Select
              value={fileInto.mailbox}
              aria-label={strings.filterFolderLabel}
              onChange={(e) => setFolder(e.target.value)}
            >
              {folders.map((m) => (
                <option key={m.id} value={m.name}>
                  {m.name}
                </option>
              ))}
            </Select>
          )}
        </div>
        <Checkbox
          checked={has("markRead")}
          disabled={has("delete")}
          onChange={() => toggleAction({ type: "markRead" })}
          label={strings.filterActionMarkRead}
        />
        <Checkbox
          checked={has("star")}
          disabled={has("delete")}
          onChange={() => toggleAction({ type: "star" })}
          label={strings.filterActionStar}
        />
        <Checkbox
          checked={has("delete")}
          onChange={() => toggleAction({ type: "delete" })}
          label={strings.filterActionDelete}
        />
      </div>

      <div className={styles.editorFoot}>
        <Button variant="ghost" onClick={onCancel}>
          {strings.filterCancel}
        </Button>
        <Button onClick={onSave} disabled={busy}>
          {busy ? <Spinner size={15} /> : strings.filterSaveRule}
        </Button>
      </div>
    </div>
  );
}
