// alo Base — the editor (ADR 0032). A relational table with typed fields and
// many views over the same records (grid / board / calendar / gallery). Every
// write is gated server-side by the Base's Drive access; this UI reflects it.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Calendar,
  CalendarDays,
  CheckSquare,
  ChevronDownCircle,
  Hash,
  LayoutGrid,
  Link2,
  Paperclip,
  Plus,
  Rows3,
  Table2,
  Tags,
  Trello,
  Type,
  User,
  X,
  type LucideIcon,
} from "lucide-react";

import { strings } from "../i18n";
import {
  useJmapClient,
  type BaseDto,
  type BaseFieldType,
  type BaseRecordDto,
  type BaseTableDto,
  type BaseViewKind,
} from "../jmap";
import { Spinner } from "../ds";
import { Cell } from "./BaseCell";
import { BoardView, CalendarView, GalleryView } from "./BaseViews";
import styles from "./BaseEditor.module.css";

const FIELD_TYPES: { type: BaseFieldType; label: () => string }[] = [
  { type: "text", label: () => strings.baseTypeText },
  { type: "number", label: () => strings.baseTypeNumber },
  { type: "date", label: () => strings.baseTypeDate },
  { type: "checkbox", label: () => strings.baseTypeCheckbox },
  { type: "select", label: () => strings.baseTypeSelect },
  { type: "multiselect", label: () => strings.baseTypeMultiselect },
  { type: "person", label: () => strings.baseTypePerson },
  { type: "link", label: () => strings.baseTypeLink },
];

/** The small type icon shown in each column header — Airtable's field-type cue. */
const FIELD_ICON: Record<BaseFieldType, LucideIcon> = {
  text: Type,
  number: Hash,
  date: Calendar,
  checkbox: CheckSquare,
  select: ChevronDownCircle,
  multiselect: Tags,
  person: User,
  link: Link2,
  attachment: Paperclip,
};

const VIEW_KINDS: { kind: BaseViewKind; label: () => string; Icon: typeof LayoutGrid }[] = [
  { kind: "grid", label: () => strings.baseViewGrid, Icon: LayoutGrid },
  { kind: "board", label: () => strings.baseViewBoard, Icon: Trello },
  { kind: "calendar", label: () => strings.baseViewCalendar, Icon: CalendarDays },
  { kind: "gallery", label: () => strings.baseViewGallery, Icon: Rows3 },
];

export function BaseEditor({
  nodeId,
  name,
  onClose,
}: {
  nodeId: string;
  name: string;
  onClose: () => void;
}) {
  const client = useJmapClient();
  const [base, setBase] = useState<BaseDto | null>(null);
  const [activeTable, setActiveTable] = useState(0);
  const [activeView, setActiveView] = useState(0);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved">("idle");

  const [fieldMenu, setFieldMenu] = useState(false);
  const [fName, setFName] = useState("");
  const [fType, setFType] = useState<BaseFieldType>("text");
  const [fChoices, setFChoices] = useState("");
  const [fLinkTable, setFLinkTable] = useState("");
  const fieldRef = useRef<HTMLDivElement>(null);

  const [viewMenu, setViewMenu] = useState(false);
  const [vKind, setVKind] = useState<BaseViewKind>("board");
  const [vField, setVField] = useState("");
  const viewRef = useRef<HTMLDivElement>(null);

  const reload = useCallback(async () => {
    try {
      setBase(await client.base(nodeId));
    } catch {
      setBase({ nodeId, tables: [] });
    }
  }, [client, nodeId]);
  useEffect(() => {
    void reload();
  }, [reload]);

  useOutside(fieldRef, fieldMenu, () => setFieldMenu(false));
  useOutside(viewRef, viewMenu, () => setViewMenu(false));

  const table: BaseTableDto | undefined = base?.tables[activeTable];
  const view = table?.views[activeView];
  const tables = base?.tables ?? [];

  async function setCellById(recordId: string, fieldId: string, value: unknown) {
    const rec = table?.records.find((r) => r.id === recordId);
    if (rec) await setCell(rec, fieldId, value);
  }

  async function setCell(record: BaseRecordDto, fieldId: string, value: unknown) {
    const cells = { ...record.cells, [fieldId]: value };
    setBase((b) =>
      b === null
        ? b
        : {
            ...b,
            tables: b.tables.map((t, i) =>
              i !== activeTable
                ? t
                : { ...t, records: t.records.map((r) => (r.id === record.id ? { ...r, cells } : r)) },
            ),
          },
    );
    setSaveState("saving");
    try {
      await client.baseUpdateRecord(record.id, cells);
      setSaveState("saved");
    } catch {
      setSaveState("idle");
      void reload();
    }
  }

  async function addRow(preset?: Record<string, unknown>) {
    if (!table) return;
    try {
      await client.baseAddRecord(table.id, preset);
      await reload();
    } catch {
      /* ignore */
    }
  }

  async function addField() {
    const nm = fName.trim();
    if (nm === "" || !table) return;
    const options: Record<string, unknown> = {};
    if (fType === "select" || fType === "multiselect") {
      options.choices = fChoices.split(",").map((c) => c.trim()).filter((c) => c !== "");
    }
    if (fType === "link" && fLinkTable !== "") options.linkTableId = fLinkTable;
    try {
      await client.baseAddField(table.id, nm, fType, options);
      setFName("");
      setFType("text");
      setFChoices("");
      setFLinkTable("");
      setFieldMenu(false);
      await reload();
    } catch {
      /* ignore */
    }
  }

  async function addTable() {
    if (!base) return;
    const idx = base.tables.length;
    try {
      await client.baseAddTable(nodeId, `Table ${idx + 1}`);
      await reload();
      setActiveTable(idx);
      setActiveView(0);
    } catch {
      /* ignore */
    }
  }

  async function addView() {
    if (!table) return;
    const config: Record<string, unknown> = {};
    if (vKind === "board") config.groupFieldId = vField;
    if (vKind === "calendar") config.dateFieldId = vField;
    const label = VIEW_KINDS.find((v) => v.kind === vKind)?.label() ?? "View";
    try {
      await client.baseAddView(table.id, vKind, label, config);
      setViewMenu(false);
      await reload();
      setActiveView(table.views.length); // the new one
    } catch {
      /* ignore */
    }
  }

  const selectFields = useMemo(
    () => table?.fields.filter((f) => f.type === "select" || f.type === "multiselect") ?? [],
    [table],
  );
  const dateFields = useMemo(() => table?.fields.filter((f) => f.type === "date") ?? [], [table]);

  return (
    <div className={styles.overlay}>
      <header className={styles.head}>
        <button type="button" className={styles.back} onClick={onClose} aria-label={strings.close}>
          <X size={18} />
        </button>
        <span className={styles.name}>{name}</span>
        <span className={styles.save}>
          {saveState === "saving" ? strings.docSaving : saveState === "saved" ? strings.docSaved : ""}
        </span>
      </header>

      {base === null ? (
        <div className={styles.center}>
          <Spinner size={22} />
        </div>
      ) : (
        <>
          <div className={styles.tabs}>
            {base.tables.map((t, i) => (
              <button
                key={t.id}
                type="button"
                className={i === activeTable ? `${styles.tab} ${styles.tabOn}` : styles.tab}
                onClick={() => {
                  setActiveTable(i);
                  setActiveView(0);
                }}
              >
                <Table2 size={14} /> {t.name}
              </button>
            ))}
            <button type="button" className={styles.tabAdd} onClick={() => void addTable()} aria-label={strings.baseNewTable}>
              <Plus size={14} />
            </button>
          </div>

          {table && (
            <div className={styles.viewbar}>
              {table.views.map((v, i) => {
                const Icon = VIEW_KINDS.find((k) => k.kind === v.kind)?.Icon ?? LayoutGrid;
                return (
                  <button
                    key={v.id}
                    type="button"
                    className={i === activeView ? `${styles.viewTab} ${styles.viewTabOn}` : styles.viewTab}
                    onClick={() => setActiveView(i)}
                  >
                    <Icon size={13} /> {v.name}
                  </button>
                );
              })}
              <div className={styles.addViewWrap} ref={viewRef}>
                <button type="button" className={styles.tabAdd} onClick={() => setViewMenu((v) => !v)} aria-label={strings.baseAddView}>
                  <Plus size={14} />
                </button>
                {viewMenu && (
                  <div className={styles.menu}>
                    <select className={styles.menuSelect} value={vKind} onChange={(e) => setVKind(e.target.value as BaseViewKind)}>
                      {VIEW_KINDS.map((k) => (
                        <option key={k.kind} value={k.kind}>
                          {k.label()}
                        </option>
                      ))}
                    </select>
                    {vKind === "board" && (
                      <select className={styles.menuSelect} value={vField} onChange={(e) => setVField(e.target.value)}>
                        <option value="">{strings.baseGroupBy}</option>
                        {selectFields.map((f) => (
                          <option key={f.id} value={f.id}>
                            {f.name}
                          </option>
                        ))}
                      </select>
                    )}
                    {vKind === "calendar" && (
                      <select className={styles.menuSelect} value={vField} onChange={(e) => setVField(e.target.value)}>
                        <option value="">{strings.baseByDate}</option>
                        {dateFields.map((f) => (
                          <option key={f.id} value={f.id}>
                            {f.name}
                          </option>
                        ))}
                      </select>
                    )}
                    <button type="button" className={styles.menuBtn} onClick={() => void addView()}>
                      {strings.baseAddView}
                    </button>
                  </div>
                )}
              </div>
            </div>
          )}

          {table && view && view.kind === "board" && (
            <div className={styles.viewBody}>
              <BoardView table={table} tables={tables} config={view.config} onSetCell={(id, fid, v) => void setCellById(id, fid, v)} onAddRow={(p) => void addRow(p)} />
            </div>
          )}
          {table && view && view.kind === "calendar" && (
            <div className={styles.viewBody}>
              <CalendarView table={table} tables={tables} config={view.config} onSetCell={(id, fid, v) => void setCellById(id, fid, v)} onAddRow={(p) => void addRow(p)} />
            </div>
          )}
          {table && view && view.kind === "gallery" && (
            <div className={styles.viewBody}>
              <GalleryView table={table} tables={tables} config={view.config} onSetCell={(id, fid, v) => void setCellById(id, fid, v)} onAddRow={(p) => void addRow(p)} />
            </div>
          )}
          {table && view && view.kind === "grid" && (
            <div className={styles.gridWrap}>
              <table className={styles.grid}>
                <thead>
                  <tr>
                    <th className={styles.rowNumHead}>#</th>
                    {table.fields.map((f, fi) => {
                      const Icon = FIELD_ICON[f.type] ?? Type;
                      return (
                        <th
                          key={f.id}
                          className={fi === 0 ? `${styles.colHead} ${styles.colHeadPrimary}` : styles.colHead}
                        >
                          <Icon size={14} className={styles.colTypeIcon} strokeWidth={1.75} />
                          <span className={styles.colName}>{f.name}</span>
                        </th>
                      );
                    })}
                    <th className={styles.addColHead}>
                      <div className={styles.addColWrap} ref={fieldRef}>
                        <button type="button" className={styles.addCol} onClick={() => setFieldMenu((v) => !v)} aria-label={strings.baseAddField}>
                          <Plus size={15} />
                        </button>
                        {fieldMenu && (
                          <div className={styles.menu}>
                            <input className={styles.menuInput} autoFocus value={fName} placeholder={strings.baseFieldName} onChange={(e) => setFName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && fType !== "select" && fType !== "multiselect" && fType !== "link" && void addField()} />
                            <select className={styles.menuSelect} value={fType} onChange={(e) => setFType(e.target.value as BaseFieldType)}>
                              {FIELD_TYPES.map((t) => (
                                <option key={t.type} value={t.type}>
                                  {t.label()}
                                </option>
                              ))}
                            </select>
                            {(fType === "select" || fType === "multiselect") && (
                              <input className={styles.menuInput} value={fChoices} placeholder={strings.baseChoicesPlaceholder} onChange={(e) => setFChoices(e.target.value)} />
                            )}
                            {fType === "link" && (
                              <select className={styles.menuSelect} value={fLinkTable} onChange={(e) => setFLinkTable(e.target.value)}>
                                <option value="">{strings.baseLinkTarget}</option>
                                {base.tables.filter((t) => t.id !== table.id).map((t) => (
                                  <option key={t.id} value={t.id}>
                                    {t.name}
                                  </option>
                                ))}
                              </select>
                            )}
                            <button type="button" className={styles.menuBtn} onClick={() => void addField()}>
                              {strings.baseAddField}
                            </button>
                          </div>
                        )}
                      </div>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {table.records.map((r, ri) => (
                    <tr key={r.id} className={styles.row}>
                      <td className={styles.rowNum}>{ri + 1}</td>
                      {table.fields.map((f, fi) => (
                        <td key={f.id} className={fi === 0 ? `${styles.cell} ${styles.cellPrimary}` : styles.cell}>
                          <Cell field={f} value={r.cells[f.id]} tables={tables} onCommit={(v) => void setCell(r, f.id, v)} />
                        </td>
                      ))}
                      <td className={styles.cellPad} />
                    </tr>
                  ))}
                </tbody>
              </table>
              <button type="button" className={styles.addRow} onClick={() => void addRow()}>
                <Plus size={15} /> {strings.baseNewRow}
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

/** Close a popover on an outside pointer-down. */
function useOutside(ref: React.RefObject<HTMLElement | null>, open: boolean, close: () => void) {
  useEffect(() => {
    if (!open) return undefined;
    function down(e: PointerEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) close();
    }
    document.addEventListener("pointerdown", down);
    return () => document.removeEventListener("pointerdown", down);
  }, [ref, open, close]);
}
