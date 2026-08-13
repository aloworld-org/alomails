// alo Base cell editors (ADR 0032) — one per field type, inline-editable. The
// relational types (select, link) are what make a Base more than a spreadsheet:
// select drives the board view; link references records in another table.
import { useEffect, useRef, useState } from "react";
import { ChevronDown, Plus, X } from "lucide-react";

import { Chip, Input } from "../ds";
import { strings } from "../i18n";
import type { BaseFieldDto, BaseRecordDto, BaseTableDto } from "../jmap";
import styles from "./BaseEditor.module.css";

/** A stable, calm colour per choice/value. */
const CHIP_COLORS = [
  "#e76f51", "#4b83c4", "#2e8b57", "#9b6dd6", "#c9a227", "#3aa6a6", "#d16587", "#6a7688",
];
export function chipColor(s: string): string {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  return CHIP_COLORS[h % CHIP_COLORS.length] ?? "#6a7688";
}

function choicesOf(field: BaseFieldDto): string[] {
  const c = field.options.choices;
  return Array.isArray(c) ? c.filter((x): x is string => typeof x === "string") : [];
}

/** The primary display value of a record (its first text-ish field), for links
 *  and board/gallery cards. */
export function recordLabel(table: BaseTableDto, record: BaseRecordDto): string {
  const first = table.fields[0];
  const v = first ? record.cells[first.id] : undefined;
  const s = v === null || v === undefined ? "" : String(v);
  return s.trim() === "" ? strings.baseUntitledRecord : s;
}

/** Read-only rendering of a value by field type — for board / gallery / calendar
 *  cards, where cells are shown, not edited. */
export function CellDisplay({
  field,
  value,
  tables,
}: {
  field: BaseFieldDto;
  value: unknown;
  tables: BaseTableDto[];
}) {
  if (value === null || value === undefined || value === "") return null;
  if (field.type === "checkbox") return value === true ? <span>✓</span> : null;
  if (field.type === "select" && typeof value === "string") {
    return <Chip color={chipColor(value)}>{value}</Chip>;
  }
  if ((field.type === "multiselect" || field.type === "link") && Array.isArray(value)) {
    const target = field.type === "link" ? tables.find((t) => t.id === field.options.linkTableId) : undefined;
    return (
      <span className={styles.chipRow}>
        {value.map((v, i) => {
          const s =
            field.type === "link" && target
              ? (() => {
                  const r = target.records.find((x) => x.id === v);
                  return r ? recordLabel(target, r) : "…";
                })()
              : String(v);
          // A link chip names a record in another table; its colour would say
          // nothing, so it stays the neutral chip.
          return field.type === "link" ? (
            <Chip key={i}>{s}</Chip>
          ) : (
            <Chip key={i} color={chipColor(s)}>{s}</Chip>
          );
        })}
      </span>
    );
  }
  return <span>{String(value)}</span>;
}

export function Cell({
  field,
  value,
  tables,
  onCommit,
}: {
  field: BaseFieldDto;
  value: unknown;
  tables: BaseTableDto[];
  onCommit: (value: unknown) => void;
}) {
  switch (field.type) {
    case "checkbox":
      return (
        <input
          type="checkbox"
          className={styles.check}
          checked={value === true}
          onChange={(e) => onCommit(e.target.checked)}
        />
      );
    case "number":
      return (
        <Input
          type="number"
          variant="cell"
          aria-label={field.name}
          defaultValue={typeof value === "number" ? value : (value as string) ?? ""}
          onBlur={(e) => onCommit(e.target.value === "" ? null : Number(e.target.value))}
        />
      );
    case "date":
      return (
        <Input
          type="date"
          variant="cell"
          aria-label={field.name}
          defaultValue={typeof value === "string" ? value : ""}
          onChange={(e) => onCommit(e.target.value || null)}
        />
      );
    case "person":
      return (
        <Input
          type="text"
          variant="cell"
          aria-label={field.name}
          inputMode="email"
          defaultValue={typeof value === "string" ? value : ""}
          placeholder={strings.basePersonPlaceholder}
          onBlur={(e) => onCommit(e.target.value || null)}
        />
      );
    case "select":
      return <SelectCell choices={choicesOf(field)} value={value} onCommit={onCommit} />;
    case "multiselect":
      return <MultiSelectCell choices={choicesOf(field)} value={value} onCommit={onCommit} />;
    case "link":
      return (
        <LinkCell
          value={value}
          target={tables.find((t) => t.id === field.options.linkTableId)}
          onCommit={onCommit}
        />
      );
    default:
      return (
        <Input
          type="text"
          variant="cell"
          aria-label={field.name}
          defaultValue={value === null || value === undefined ? "" : String(value)}
          onBlur={(e) => onCommit(e.target.value)}
        />
      );
  }
}

function SelectCell({
  choices,
  value,
  onCommit,
}: {
  choices: string[];
  value: unknown;
  onCommit: (v: unknown) => void;
}) {
  const current = typeof value === "string" ? value : "";
  return (
    <div className={styles.selectCell}>
      {current !== "" && <Chip color={chipColor(current)}>{current}</Chip>}
      <select
        className={styles.selectHidden}
        value={current}
        onChange={(e) => onCommit(e.target.value || null)}
      >
        <option value="">—</option>
        {choices.map((c) => (
          <option key={c} value={c}>
            {c}
          </option>
        ))}
      </select>
      <ChevronDown size={13} className={styles.selectChev} />
    </div>
  );
}

function MultiSelectCell({
  choices,
  value,
  onCommit,
}: {
  choices: string[];
  value: unknown;
  onCommit: (v: unknown) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const selected = Array.isArray(value) ? value.filter((x): x is string => typeof x === "string") : [];
  useEffect(() => {
    if (!open) return undefined;
    function down(e: PointerEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("pointerdown", down);
    return () => document.removeEventListener("pointerdown", down);
  }, [open]);
  function toggle(c: string) {
    onCommit(selected.includes(c) ? selected.filter((s) => s !== c) : [...selected, c]);
  }
  return (
    <div className={styles.linkCell} ref={ref}>
      <button type="button" className={styles.chipsBtn} onClick={() => setOpen((v) => !v)}>
        {selected.length === 0 ? (
          <span className={styles.cellEmpty}>—</span>
        ) : (
          selected.map((c) => (
            <Chip key={c} color={chipColor(c)}>
              {c}
            </Chip>
          ))
        )}
      </button>
      {open && (
        <div className={styles.pickerMenu}>
          {choices.length === 0 && <div className={styles.pickerEmpty}>{strings.baseNoChoices}</div>}
          {choices.map((c) => (
            <button key={c} type="button" className={styles.pickerRow} onClick={() => toggle(c)}>
              <Chip color={chipColor(c)}>{c}</Chip>
              {selected.includes(c) && <X size={13} />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function LinkCell({
  value,
  target,
  onCommit,
}: {
  value: unknown;
  target: BaseTableDto | undefined;
  onCommit: (v: unknown) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const ids = Array.isArray(value) ? value.filter((x): x is string => typeof x === "string") : [];
  useEffect(() => {
    if (!open) return undefined;
    function down(e: PointerEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("pointerdown", down);
    return () => document.removeEventListener("pointerdown", down);
  }, [open]);
  const nameOf = (id: string) => {
    const rec = target?.records.find((r) => r.id === id);
    return rec && target ? recordLabel(target, rec) : "…";
  };
  function toggle(id: string) {
    onCommit(ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]);
  }
  return (
    <div className={styles.linkCell} ref={ref}>
      <button type="button" className={styles.chipsBtn} onClick={() => setOpen((v) => !v)}>
        {ids.length === 0 ? (
          <span className={styles.cellEmpty}>
            <Plus size={13} /> {strings.baseLink}
          </span>
        ) : (
          ids.map((id) => <Chip key={id}>{nameOf(id)}</Chip>)
        )}
      </button>
      {open && (
        <div className={styles.pickerMenu}>
          {target === undefined ? (
            <div className={styles.pickerEmpty}>{strings.baseLinkNoTable}</div>
          ) : target.records.length === 0 ? (
            <div className={styles.pickerEmpty}>{strings.baseLinkNoRecords}</div>
          ) : (
            target.records.map((r) => (
              <button key={r.id} type="button" className={styles.pickerRow} onClick={() => toggle(r.id)}>
                <span>{recordLabel(target, r)}</span>
                {ids.includes(r.id) && <X size={13} />}
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}
