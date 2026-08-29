// alo Sheet — a native spreadsheet on Univer (Apache-2.0, no third-party
// branding). Like alo Doc, a sheet is a Drive node (kind "sheet") whose content
// is the editor's own JSON snapshot stored in the node's blob; opening loads it,
// edits auto-save a new version. Univer is heavy, so DriveModule code-splits this
// out — it loads only when a sheet is opened.
import { Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Bot, Check, ChevronLeft, Download, MoreHorizontal, Pencil, Trash2, X } from "lucide-react";

// Univer's own UI + engine. Framework-agnostic: it mounts into a plain DOM
// container we hand it, so we drive it from an effect rather than as JSX.
import { createUniver, LocaleType, merge, defaultTheme } from "@univerjs/presets";
import { CommandType, type IBorderData, type IRange, type IStyleData } from "@univerjs/core";
import {
  ALL_IMPLEMENTED_FUNCTIONS_SET,
  FUNCTION_NAMES_ARRAY,
  FUNCTION_NAMES_COMPATIBILITY,
  FUNCTION_NAMES_CUBE,
  FUNCTION_NAMES_DATABASE,
  FUNCTION_NAMES_DATE,
  FUNCTION_NAMES_ENGINEERING,
  FUNCTION_NAMES_FINANCIAL,
  FUNCTION_NAMES_INFORMATION,
  FUNCTION_NAMES_LOGICAL,
  FUNCTION_NAMES_LOOKUP,
  FUNCTION_NAMES_MATH,
  FUNCTION_NAMES_STATISTICAL,
  FUNCTION_NAMES_TEXT,
  FUNCTION_NAMES_WEB,
} from "@univerjs/engine-formula";
import { UniverSheetsCorePreset } from "@univerjs/presets/preset-sheets-core";
import sheetsCoreEnUS from "@univerjs/presets/preset-sheets-core/locales/en-US";
import { UniverSheetsSortPreset } from "@univerjs/presets/preset-sheets-sort";
import sheetsSortEnUS from "@univerjs/presets/preset-sheets-sort/locales/en-US";
import { UniverSheetsFilterPreset } from "@univerjs/presets/preset-sheets-filter";
import sheetsFilterEnUS from "@univerjs/presets/preset-sheets-filter/locales/en-US";
import { UniverSheetsFindReplacePreset } from "@univerjs/presets/preset-sheets-find-replace";
import sheetsFindReplaceEnUS from "@univerjs/presets/preset-sheets-find-replace/locales/en-US";
import { UniverSheetsConditionalFormattingPreset } from "@univerjs/presets/preset-sheets-conditional-formatting";
import sheetsConditionalFormattingEnUS from "@univerjs/presets/preset-sheets-conditional-formatting/locales/en-US";
import { UniverSheetsDataValidationPreset } from "@univerjs/presets/preset-sheets-data-validation";
import sheetsDataValidationEnUS from "@univerjs/presets/preset-sheets-data-validation/locales/en-US";
import { UniverSheetsDrawingPreset } from "@univerjs/presets/preset-sheets-drawing";
import sheetsDrawingEnUS from "@univerjs/presets/preset-sheets-drawing/locales/en-US";
import { UniverSheetsHyperLinkPreset } from "@univerjs/presets/preset-sheets-hyper-link";
import sheetsHyperLinkEnUS from "@univerjs/presets/preset-sheets-hyper-link/locales/en-US";
import { UniverSheetsNotePreset } from "@univerjs/presets/preset-sheets-note";
import sheetsNoteEnUS from "@univerjs/presets/preset-sheets-note/locales/en-US";
import { UniverSheetsTablePreset } from "@univerjs/presets/preset-sheets-table";
import sheetsTableEnUS from "@univerjs/presets/preset-sheets-table/locales/en-US";
import { UniverSheetsThreadCommentPreset } from "@univerjs/presets/preset-sheets-thread-comment";
import sheetsThreadCommentEnUS from "@univerjs/presets/preset-sheets-thread-comment/locales/en-US";
import "@univerjs/presets/lib/styles/preset-sheets-core.css";
import "@univerjs/presets/lib/styles/preset-sheets-sort.css";
import "@univerjs/presets/lib/styles/preset-sheets-filter.css";
import "@univerjs/presets/lib/styles/preset-sheets-find-replace.css";
import "@univerjs/presets/lib/styles/preset-sheets-conditional-formatting.css";
import "@univerjs/presets/lib/styles/preset-sheets-data-validation.css";
import "@univerjs/presets/lib/styles/preset-sheets-drawing.css";
import "@univerjs/presets/lib/styles/preset-sheets-hyper-link.css";
import "@univerjs/presets/lib/styles/preset-sheets-note.css";
import "@univerjs/presets/lib/styles/preset-sheets-table.css";
import "@univerjs/presets/lib/styles/preset-sheets-thread-comment.css";

import { RecordAgentPanel, type RecordOrigin } from "../agents";
import { strings } from "../i18n";
import { Chart } from "../insights/chart";
import { useJmapClient } from "../jmap";
import { Menu, Spinner } from "../ds";
import { driveErrorReason, saveBlob } from "./parts";
import { univerSnapshotToXlsx } from "./exportOffice";
import {
  readSheetDocument,
  writeSheetDocument,
  type SheetChart,
  type SheetChartKind,
  type Snapshot as SheetSnapshot,
} from "./sheetDocument";
import { rangeReference, sheetChartModel } from "./sheetChartModel";
import { SheetRibbon, type BorderKind, type FormulaCategory, type SheetActions, type SheetSelectionFormatting } from "./SheetRibbon";
import styles from "./SheetEditor.module.css";

// Keep Univer's engine chrome aligned with alo's terracotta brand. Univer uses
// `primary` for selections/focus and `blue` directly in a few legacy menus and
// sheet controls, so both palettes intentionally point to the same scale.
const ALO_ORANGE = {
  50: "#fceee9",
  100: "#f8d6cc",
  200: "#f1bdad",
  300: "#eca18b",
  400: "#ea8368",
  500: "#e76f51",
  600: "#d65b3e",
  700: "#b84a32",
  800: "#8f3a28",
  900: "#6f2b1e",
};
const ALO_SHEET_THEME = {
  ...defaultTheme,
  primary: ALO_ORANGE,
  blue: ALO_ORANGE,
  orange: ALO_ORANGE,
};
const STYLE_STATE_PRESETS = [
  { key: "normal", size: 11, bold: false },
  { key: "h1", size: 20, bold: true },
  { key: "h2", size: 16, bold: true },
  { key: "h3", size: 14, bold: true },
  { key: "h4", size: 12, bold: true },
  { key: "title", size: 28, bold: true },
  { key: "subtitle", size: 15, bold: false },
] as const;

const XLSX_MIME = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

type SaveState = "idle" | "saving" | "saved";

type BorderRangeReader = { getRange: () => IRange };
type BorderSheetReader = { getRange: (row: number, column: number) => { getCellStyleData: (type?: "row" | "col" | "cell") => IStyleData | null } };

function detectBorderKind(range: BorderRangeReader, worksheet: BorderSheetReader): BorderKind | null {
  const selected = range.getRange();
  const rows = selected.endRow - selected.startRow + 1;
  const columns = selected.endColumn - selected.startColumn + 1;
  if (rows <= 0 || columns <= 0 || rows * columns > 10_000) return null;
  const borders: IBorderData[][] = Array.from({ length: rows }, (_row, r) =>
    Array.from({ length: columns }, (_column, c) => worksheet.getRange(selected.startRow + r, selected.startColumn + c).getCellStyleData("cell")?.bd ?? {}),
  );
  const has = (row: number, column: number, edge: "t" | "r" | "b" | "l") => borders[row]?.[column]?.[edge] != null;
  const every = (test: (row: number, column: number) => boolean) => borders.every((row, r) => row.every((_cell, c) => test(r, c)));
  const bd = borders[0]?.[0];
  if (bd?.tl_br != null) return "diag-down";
  if (bd?.bl_tr != null) return "diag-up";
  if (every((r, c) => has(r, c, "t") && has(r, c, "r") && has(r, c, "b") && has(r, c, "l"))) return "all";
  const top = Array.from({ length: columns }, (_, c) => has(0, c, "t")).every(Boolean);
  const bottom = Array.from({ length: columns }, (_, c) => has(rows - 1, c, "b")).every(Boolean);
  const left = Array.from({ length: rows }, (_, r) => has(r, 0, "l")).every(Boolean);
  const right = Array.from({ length: rows }, (_, r) => has(r, columns - 1, "r")).every(Boolean);
  const horizontal = rows > 1 && Array.from({ length: rows - 1 }, (_, r) => Array.from({ length: columns }, (_unused, c) => has(r, c, "b") || has(r + 1, c, "t")).every(Boolean)).every(Boolean);
  const vertical = columns > 1 && Array.from({ length: columns - 1 }, (_, c) => Array.from({ length: rows }, (_unused, r) => has(r, c, "r") || has(r, c + 1, "l")).every(Boolean)).every(Boolean);
  if (top && bottom && left && right) return "outer";
  if (horizontal && vertical) return "inside";
  if (top) return "top";
  if (bottom) return "bottom";
  if (left) return "left";
  if (right) return "right";
  if (horizontal) return "horizontal";
  if (vertical) return "vertical";
  return null;
}

function supportedFunctions(values: object): string[] {
  return Object.values(values).filter((name): name is string => typeof name === "string" && ALL_IMPLEMENTED_FUNCTIONS_SET.has(name)).sort();
}

const FORMULA_CATEGORIES: FormulaCategory[] = [
  { key: "financial", label: strings.sheetFormulaFinancial, functions: supportedFunctions(FUNCTION_NAMES_FINANCIAL) },
  { key: "date", label: strings.sheetFormulaDateTime, functions: supportedFunctions(FUNCTION_NAMES_DATE) },
  { key: "math", label: strings.sheetFormulaMathTrig, functions: supportedFunctions(FUNCTION_NAMES_MATH) },
  { key: "statistical", label: strings.sheetFormulaStatistical, functions: supportedFunctions(FUNCTION_NAMES_STATISTICAL) },
  { key: "lookup", label: strings.sheetFormulaLookup, functions: supportedFunctions(FUNCTION_NAMES_LOOKUP) },
  { key: "database", label: strings.sheetFormulaDatabase, functions: supportedFunctions(FUNCTION_NAMES_DATABASE) },
  { key: "text", label: strings.sheetFormulaText, functions: supportedFunctions(FUNCTION_NAMES_TEXT) },
  { key: "logical", label: strings.sheetFormulaLogical, functions: supportedFunctions(FUNCTION_NAMES_LOGICAL) },
  { key: "information", label: strings.sheetFormulaInformation, functions: supportedFunctions(FUNCTION_NAMES_INFORMATION) },
  { key: "engineering", label: strings.sheetFormulaEngineering, functions: supportedFunctions(FUNCTION_NAMES_ENGINEERING) },
  { key: "cube", label: strings.sheetFormulaCube, functions: supportedFunctions(FUNCTION_NAMES_CUBE) },
  { key: "compatibility", label: strings.sheetFormulaCompatibility, functions: supportedFunctions(FUNCTION_NAMES_COMPATIBILITY) },
  { key: "web", label: strings.sheetFormulaWeb, functions: supportedFunctions(FUNCTION_NAMES_WEB) },
  { key: "array", label: strings.sheetFormulaArray, functions: supportedFunctions(FUNCTION_NAMES_ARRAY) },
].filter((category) => category.functions.length > 0);

/** A Univer workbook snapshot — an opaque JSON object we persist verbatim.
 *  The *stored* blob is an envelope around it (ADR 0051): the engine
 *  regenerates this object from its own state on every save, so alo's charts
 *  live beside it rather than inside it. `sheetDocument.ts` owns both shapes. */
type Snapshot = SheetSnapshot;

export function SheetEditor({
  nodeId,
  name,
  origin = null,
  onNameChange,
  onClose,
}: {
  nodeId: string;
  name: string;
  /** Where this workbook came from, as Drive carries it; `null` when it does
   *  not say. Passed in rather than read again — the file list already had
   *  the node (A8.4). */
  origin?: RecordOrigin | null;
  onNameChange: (name: string) => void;
  onClose: () => void;
}) {
  const client = useJmapClient();
  const containerRef = useRef<HTMLDivElement>(null);
  const apiRef = useRef<ReturnType<typeof createUniver>["univerAPI"] | null>(null);
  const disposeRef = useRef<(() => void) | null>(null);
  const lastSaved = useRef<string>("");
  const dirtyRef = useRef(false);
  const savingRef = useRef(false);
  const [initial, setInitial] = useState<Snapshot | null>(null);
  /** The charts read from the stored envelope, carried across saves. A ref
   *  rather than state: nothing here re-renders on them yet, and losing them
   *  to a stale closure in the auto-save interval would delete a chart. */
  const chartsRef = useRef<SheetChart[]>([]);
  const [charts, setCharts] = useState<SheetChart[]>([]);
  // The workbook's agent, in the same right-hand rail the charts use. Closed
  // until asked for (ADR 0057): opening it is what makes its two reads.
  const [agentOpen, setAgentOpen] = useState(false);
  const [chartWorkbook, setChartWorkbook] = useState<SheetSnapshot>({});
  const sheetBlob = useCallback(
    (json: string) =>
      writeSheetDocument({
        workbook: JSON.parse(json) as Snapshot,
        charts: chartsRef.current,
      }),
    [],
  );
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [actionError, setActionError] = useState("");
  const [ready, setReady] = useState(false);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [activeBorder, setActiveBorder] = useState<BorderKind | null>(null);
  const [selectionFormatting, setSelectionFormatting] = useState<SheetSelectionFormatting>({ activeKeys: [], fontFamily: null, fontSize: null, numberPattern: null, fontColor: "#000000", fillColor: "#ffffff" });
  // Editable sheet name (persisted via driveRename); the grid filename tracks it.
  const [sheetName, setSheetName] = useState(name);
  const nameRef = useRef<HTMLInputElement>(null);

  // Load the stored snapshot (or an empty workbook) before mounting Univer.
  useEffect(() => {
    let live = true;
    setInitial(null);
    setReady(false);
    setLoadError(null);
    void client
      .driveSheetContent(nodeId)
      .then((data) => {
        if (!live) return;
        // Either shape: an envelope hands back its workbook, a bare snapshot
        // is its own. Charts are held so a save puts them back — an editor
        // that dropped them would silently delete a chart on the next
        // keystroke.
        const stored = readSheetDocument(data ?? {});
        chartsRef.current = stored.charts;
        setCharts(stored.charts);
        setChartWorkbook(stored.workbook);
        setInitial(stored.workbook);
      })
      .catch((error: unknown) => {
        if (live) setLoadError(driveErrorReason(error) ?? strings.driveUnknownError);
      });
    return () => {
      live = false;
    };
  }, [client, loadAttempt, nodeId]);

  // Mount Univer once the container exists and the snapshot has loaded.
  useEffect(() => {
    if (initial === null || containerRef.current === null) return undefined;
    const { univerAPI } = createUniver({
      locale: LocaleType.EN_US,
      locales: {
        [LocaleType.EN_US]: merge(
          {},
          sheetsCoreEnUS,
          sheetsSortEnUS,
          sheetsFilterEnUS,
          sheetsFindReplaceEnUS,
          sheetsConditionalFormattingEnUS,
          sheetsDataValidationEnUS,
          sheetsDrawingEnUS,
          sheetsHyperLinkEnUS,
          sheetsNoteEnUS,
          sheetsTableEnUS,
          sheetsThreadCommentEnUS,
        ),
      },
      theme: ALO_SHEET_THEME,
      // Hide Univer's built-in toolbar — our own ribbon (SheetRibbon) replaces
      // it. Keep the formula bar and grid. Extra presets power the ribbon's
      // Data/Editing tools (sort, filter, find & replace).
      presets: [
        UniverSheetsCorePreset({
          container: containerRef.current,
          toolbar: false,
          sheets: {
            scrollConfig: {
              barSize: 3,
              barBorder: 0,
              thumbMargin: 0,
              thumbBackgroundColor: "rgba(231, 111, 81, 0.78)",
              thumbHoverBackgroundColor: "#e76f51",
              thumbActiveBackgroundColor: "#d65d3f",
              trackBackgroundColor: "transparent",
              trackBorderColor: "transparent",
            },
          },
        }),
        UniverSheetsSortPreset(),
        UniverSheetsFilterPreset(),
        UniverSheetsFindReplacePreset(),
        UniverSheetsConditionalFormattingPreset(),
        UniverSheetsDataValidationPreset(),
        UniverSheetsDrawingPreset(),
        UniverSheetsHyperLinkPreset(),
        UniverSheetsNotePreset(),
        UniverSheetsTablePreset(),
        UniverSheetsThreadCommentPreset(),
      ],
    });
    apiRef.current = univerAPI;
    // An empty object → a blank default workbook; a stored snapshot → that book.
    const workbook = univerAPI.createWorkbook(Object.keys(initial).length > 0 ? initial : {});
    const updateSelectionFormatting = () => {
      const selected = workbook.getActiveRange();
      const sheet = workbook.getActiveSheet();
      if (selected === null || sheet === null) {
        setActiveBorder((current) => current === null ? current : null);
        setSelectionFormatting((current) => {
          if (current.activeKeys.length === 0 && current.fontFamily === null && current.fontSize === null && current.numberPattern === null && current.fontColor === "#000000" && current.fillColor === "#ffffff") return current;
          return { activeKeys: [], fontFamily: null, fontSize: null, numberPattern: null, fontColor: "#000000", fillColor: "#ffffff" };
        });
        return;
      }
      const nextBorder = detectBorderKind(selected, sheet);
      setActiveBorder((current) => current === nextBorder ? current : nextBorder);
      const selectedRange = selected.getRange();
      const style = sheet.getRange(selectedRange.startRow, selectedRange.startColumn).getCellStyleData("cell") ?? {};
      const activeKeys: string[] = [];
      if (style.bl === 1) activeKeys.push("bold");
      if (style.it === 1) activeKeys.push("italic");
      if (style.ul?.s === 1) activeKeys.push("underline");
      if (style.st?.s === 1) activeKeys.push("strike");
      if (style.cl !== undefined && style.cl !== null) activeKeys.push("font-color");
      if (style.bg !== undefined && style.bg !== null) activeKeys.push("fill-color");
      const horizontalValue = selected.getHorizontalAlignment();
      const horizontal = horizontalValue === "normal" ? "right" : horizontalValue;
      if (horizontal === "left" || horizontal === "center" || horizontal === "right") activeKeys.push(`align-${horizontal}`);
      const vertical = selected.getVerticalAlignment();
      if (vertical === "top" || vertical === "middle" || vertical === "bottom") activeKeys.push(`valign-${vertical}`);
      const wrap = selected.getWrapStrategy();
      if (wrap === univerAPI.Enum.WrapStrategy.OVERFLOW) activeKeys.push("wrap-overflow");
      if (wrap === univerAPI.Enum.WrapStrategy.WRAP) activeKeys.push("wrap-wrap");
      if (wrap === univerAPI.Enum.WrapStrategy.CLIP) activeKeys.push("wrap-clip");
      if (selected.isPartOfMerge()) activeKeys.push("merge-all");
      if (style.tr?.v === 1) activeKeys.push("rotation-vertical");
      else activeKeys.push(`rotation-${style.tr?.a ?? 0}`);
      const fontFamily = selected.getFontFamily();
      const fontSize = selected.getFontSize();
      const preset = STYLE_STATE_PRESETS.find((candidate) => candidate.size === fontSize && candidate.bold === (style.bl === 1));
      if (preset !== undefined) activeKeys.push(`style-${preset.key}`);
      const pattern = style.n?.pattern ?? null;
      const fontColor = style.cl?.rgb ?? "#000000";
      const fillColor = style.bg?.rgb ?? "#ffffff";
      if (pattern === "0.00%") activeKeys.push("number-percent");
      if (pattern === "€ #,##0.00") activeKeys.push("number-currency");
      if (pattern === "#,##0") activeKeys.push("number-comma");
      const formula = selected.getFormula();
      const formulaName = /^=\s*([A-Z][A-Z0-9._]*)\s*\(/i.exec(formula)?.[1]?.toUpperCase() ?? null;
      if (formulaName !== null) {
        activeKeys.push(`formula-${formulaName}`);
        const category = FORMULA_CATEGORIES.find((candidate) => candidate.functions.includes(formulaName));
        if (category !== undefined) activeKeys.push(`formula-category-${category.key}`);
      }
      const row = selectedRange.startRow;
      const column = selectedRange.startColumn;
      const rawSheet = sheet.getSheet();
      if (!rawSheet.getRowRawVisible(row)) activeKeys.push("hidden-row");
      if (!rawSheet.getColVisible(column)) activeKeys.push("hidden-column");
      if (!sheet.hasHiddenGridLines()) activeKeys.push("gridlines");
      activeKeys.push(rawSheet.isRightToLeft() === 1 ? "direction-rtl" : "direction-ltr");
      if (selected.getDataValidation() !== null) activeKeys.push("data-validation");
      if (selected.getConditionalFormattingRules().length > 0) activeKeys.push("conditional-formatting");
      if (selected.getNote() !== null) activeKeys.push("note");
      if (selected.getComment() !== null) activeKeys.push("comment");
      if (selected.getRangePermission().isProtected()) activeKeys.push("protected-range");
      if (sheet.getWorksheetPermission().isProtected()) activeKeys.push("protected-sheet");
      const frozenRows = sheet.getFrozenRows();
      const frozenColumns = sheet.getFrozenColumns();
      if (frozenRows === 1) activeKeys.push("freeze-top-row");
      if (frozenColumns === 1) activeKeys.push("freeze-first-column");
      if (frozenRows > 0 || frozenColumns > 0) activeKeys.push("freeze-panes");
      if (sheet.getZoom() === 1) activeKeys.push("zoom-100");
      setSelectionFormatting((current) => {
        const sameKeys = current.activeKeys.length === activeKeys.length && current.activeKeys.every((key, index) => key === activeKeys[index]);
        if (sameKeys && current.fontFamily === fontFamily && current.fontSize === fontSize && current.numberPattern === pattern && current.fontColor === fontColor && current.fillColor === fillColor) return current;
        return { activeKeys, fontFamily, fontSize, numberPattern: pattern, fontColor, fillColor };
      });
    };
    let formattingFrame: number | null = null;
    const scheduleFormattingUpdate = () => {
      if (formattingFrame !== null) return;
      formattingFrame = window.requestAnimationFrame(() => {
        formattingFrame = null;
        updateSelectionFormatting();
      });
    };
    const selectionSubscription = workbook.onSelectionChange(scheduleFormattingUpdate);
    const commandSubscription = workbook.onCommandExecuted((command) => {
      if (command.type === CommandType.MUTATION) dirtyRef.current = true;
      scheduleFormattingUpdate();
    });
    updateSelectionFormatting();
    lastSaved.current = snapshotJson(univerAPI);
    disposeRef.current = () => univerAPI.dispose();
    setReady(true);
    return () => {
      selectionSubscription.dispose();
      commandSubscription.dispose();
      if (formattingFrame !== null) window.cancelAnimationFrame(formattingFrame);
      disposeRef.current?.();
      apiRef.current = null;
      disposeRef.current = null;
    };
  }, [initial]);

  // Auto-save: poll the workbook snapshot and persist a new Drive version when it
  // changes. Polling avoids coupling to Univer's evolving event API.
  useEffect(() => {
    if (!ready) return undefined;
    const timer = window.setInterval(() => {
      if (!dirtyRef.current || savingRef.current) return;
      const api = apiRef.current;
      if (api === null) return;
      const json = snapshotJson(api);
      if (json === "") return;
      setChartWorkbook(JSON.parse(json) as SheetSnapshot);
      if (json === lastSaved.current) {
        dirtyRef.current = false;
        return;
      }
      dirtyRef.current = false;
      savingRef.current = true;
      setSaveState("saving");
      void client
        .driveSaveSheet(nodeId, sheetBlob(json))
        .then(() => {
          lastSaved.current = json;
          savingRef.current = false;
          setSaveState("saved");
        })
        .catch((error: unknown) => {
          savingRef.current = false;
          dirtyRef.current = true;
          setSaveState("idle");
          setActionError(strings.sheetSaveFailed(driveErrorReason(error) ?? strings.driveUnknownError));
        });
    }, 2500);
    return () => window.clearInterval(timer);
  }, [ready, client, nodeId]);

  /** Send the last edit before this editor goes away — on closing it, and on
   *  anything else navigating out of it (the agent panel opening a room). */
  function flushSave() {
    const api = apiRef.current;
    if (api === null) return;
    const json = snapshotJson(api);
    if (json !== "" && json !== lastSaved.current) {
      void client.driveSaveSheet(nodeId, sheetBlob(json));
    }
  }

  function close() {
    flushSave();
    onClose();
  }

  // Ribbon → engine bridge. The ribbon is pure UI; these implement its actions
  // against Univer's facade (the one place that touches the engine).
  const actions = useMemo<SheetActions>(() => {
    const wb = () => apiRef.current?.getActiveWorkbook() ?? null;
    const ws = () => wb()?.getActiveSheet() ?? null;
    const range = () => wb()?.getActiveRange() ?? null;
    return {
      exec: (id, params) => {
        void apiRef.current?.executeCommand(id, params);
      },
      setFontFamily: (family) => {
        range()?.setFontFamily(family);
      },
      setFontSize: (size) => {
        range()?.setFontSize(size);
      },
      adjustFontSize: (delta) => {
        const r = range();
        if (r !== null) r.setFontSize(Math.max(1, (r.getFontSize() ?? 11) + delta));
      },
      setFontColor: (hex) => {
        range()?.setFontColor(hex);
      },
      setFillColor: (hex) => {
        range()?.setBackgroundColor(hex);
      },
      setBorder: (kind) => {
        const r = range();
        const api = apiRef.current;
        if (r === null || api === null || api === undefined) return;
        const BT = api.Enum.BorderType;
        const types = {
          all: BT.ALL,
          outer: BT.OUTSIDE,
          inside: BT.INSIDE,
          top: BT.TOP,
          bottom: BT.BOTTOM,
          left: BT.LEFT,
          right: BT.RIGHT,
          horizontal: BT.HORIZONTAL,
          vertical: BT.VERTICAL,
          none: BT.NONE,
          "diag-down": BT.TLBR,
          // Univer 0.25.1 exposes BLTR as "bl_tr", while its border command
          // looks for "bltr". Pass the spelling consumed by the command.
          "diag-up": "bltr" as (typeof BT)[keyof typeof BT],
          "diag-down-center": BT.TLBC_TLMR,
          "diag-down-both": BT.TLBR_TLBC_TLMR,
          "diag-up-center": BT.MLTR_BCTR,
        } satisfies Record<Parameters<SheetActions["setBorder"]>[0], (typeof BT)[keyof typeof BT]>;
        const type = types[kind];
        // Border tools are exclusive in our ribbon: choosing a new position
        // replaces the previous border configuration on the selection instead
        // of accumulating another edge. Keep this to position commands only;
        // Univer's style/colour commands also apply stale positions.
        void (async () => {
          await api.executeCommand("sheet.command.set-border-position", { value: BT.NONE });
          if (kind !== "none") {
            await api.executeCommand("sheet.command.set-border-position", { value: type });
          }
        })();
      },
      setRotation: (rotation) => {
        const r = range();
        if (r === null) return;
        // The 0.25.1 runtime accepts a string to enable stacked vertical text,
        // although the public Facade declaration only exposes numeric angles.
        (r.setTextRotation as (value: number | string) => unknown)(rotation);
      },
      align: (a) => {
        const r = range();
        if (r === null) return;
        // Univer 0.25.1's Facade uses the misleading value `normal` for RIGHT.
        // Passing `right` throws at runtime even though the API docs describe it
        // as right alignment. Keep this compatibility detail behind SheetActions.
        r.setHorizontalAlignment(a === "right" ? "normal" : a);
      },
      valign: (a) => {
        range()?.setVerticalAlignment(a);
      },
      setWrapMode: (mode) => {
        const r = range();
        const api = apiRef.current;
        if (r === null || api === null) return;
        const strategies = {
          overflow: api.Enum.WrapStrategy.OVERFLOW,
          wrap: api.Enum.WrapStrategy.WRAP,
          clip: api.Enum.WrapStrategy.CLIP,
        };
        r.setWrapStrategy(strategies[mode]);
      },
      mergeCells: (mode) => {
        const r = range();
        if (r === null) return;
        if (mode === "all") r.merge();
        else if (mode === "across") r.mergeAcross();
        else if (mode === "vertical") r.mergeVertically();
        else r.breakApart();
      },
      setNumberFormat: (pattern) => {
        range()?.setNumberFormat(pattern);
      },
      setRowHeight: (height) => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) sheet.setRowHeight(r.getRow(), height);
      },
      setColumnWidth: (width) => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) sheet.setColumnWidth(r.getColumn(), width);
      },
      autoFitRow: () => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) sheet.autoResizeRows(r.getRow(), 1);
      },
      autoFitColumn: () => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) sheet.autoResizeColumns(r.getColumn(), 1);
      },
      hideRow: () => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) sheet.hideRows(r.getRow(), 1);
      },
      showRows: () => {
        const sheet = ws();
        if (sheet !== null) sheet.showRows(0, sheet.getMaxRows());
      },
      hideColumn: () => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) sheet.hideColumns(r.getColumn(), 1);
      },
      showColumns: () => {
        const sheet = ws();
        if (sheet !== null) sheet.showColumns(0, sheet.getMaxColumns());
      },
      toggleGridlines: () => {
        const sheet = ws();
        if (sheet !== null) sheet.setHiddenGridlines(!sheet.hasHiddenGridLines());
      },
      setGridlineColor: (hex) => {
        ws()?.setGridLinesColor(hex);
      },
      setSheetDirection: (direction) => {
        void apiRef.current?.executeCommand("sheet.command.set-worksheet-right-to-left", { rightToLeft: direction === "rtl" ? 1 : 0 });
      },
      insertRow: (where) => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) {
          const i = r.getRow();
          if (where === "before") sheet.insertRowBefore(i);
          else sheet.insertRowAfter(i);
        }
      },
      insertColumn: (where) => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) {
          const i = r.getColumn();
          if (where === "before") sheet.insertColumnBefore(i);
          else sheet.insertColumnAfter(i);
        }
      },
      deleteRow: () => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) sheet.deleteRows(r.getRow(), 1);
      },
      deleteColumn: () => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) sheet.deleteColumns(r.getColumn(), 1);
      },
      clearContents: () => {
        range()?.clearContent();
      },
      clearFormats: () => {
        range()?.clearFormat();
      },
      freezeAtSelection: () => {
        const r = range();
        const sheet = ws();
        if (r !== null && sheet !== null) {
          sheet.setFrozenRows(r.getRow());
          sheet.setFrozenColumns(r.getColumn());
        }
      },
      freezeTopRow: () => {
        ws()?.setFrozenRows(1);
      },
      freezeFirstColumn: () => {
        ws()?.setFrozenColumns(1);
      },
      unfreeze: () => {
        ws()?.cancelFreeze();
      },
      splitTextToColumns: () => {
        range()?.splitTextToColumns(true);
      },
      protectRange: () => {
        const selected = range();
        if (selected !== null && !selected.getRangePermission().isProtected()) {
          void selected.getRangePermission().protect({ name: strings.sheetProtectedRangeName });
        }
      },
      unprotectRange: () => {
        const selected = range();
        if (selected !== null) void selected.getRangePermission().unprotect();
      },
      protectSheet: () => {
        const sheet = ws();
        if (sheet !== null && !sheet.getWorksheetPermission().isProtected()) {
          void sheet.getWorksheetPermission().protect({ name: strings.sheetProtectedSheetName });
        }
      },
      unprotectSheet: () => {
        const sheet = ws();
        if (sheet !== null) void sheet.getWorksheetPermission().unprotect();
      },
      adjustZoom: (delta) => {
        const sheet = ws();
        if (sheet !== null) sheet.zoom(Math.min(4, Math.max(0.1, Math.round((sheet.getZoom() + delta) * 10) / 10)));
      },
      resetZoom: () => {
        ws()?.zoom(1);
      },
      stylePreset: ({ size, bold, color }) => {
        const r = range();
        if (r === null) return;
        r.setFontSize(size);
        r.setFontWeight(bold ? "bold" : "normal");
        if (color !== undefined) r.setFontColor(color);
      },
      undo: () => {
        void wb()?.undo();
      },
      redo: () => {
        void wb()?.redo();
      },
      addChart: (kind: SheetChartKind) => {
        const api = apiRef.current;
        const sheet = ws();
        const selected = range()?.getRange();
        if (api === null || sheet === null || selected === undefined) return;
        const startRow = Math.min(selected.startRow, selected.endRow);
        const endRow = Math.max(selected.startRow, selected.endRow);
        const startColumn = Math.min(selected.startColumn, selected.endColumn);
        const endColumn = Math.max(selected.startColumn, selected.endColumn);
        if (endRow <= startRow || endColumn <= startColumn) {
          setActionError(strings.sheetChartSelectionHint);
          return;
        }
        const json = snapshotJson(api);
        if (json === "") return;
        const workbook = JSON.parse(json) as SheetSnapshot;
        const tab = sheet.getSheetId();
        const header = (column: number) => {
          const sheets = (workbook as { sheets?: Record<string, { cellData?: Record<string, Record<string, { v?: unknown }>> }> }).sheets;
          const value = sheets?.[tab]?.cellData?.[String(startRow)]?.[String(column)]?.v;
          return value === undefined || value === null || String(value).trim() === ""
            ? strings.sheetChartSeries(column - startColumn)
            : String(value);
        };
        const columns = kind === "pie" ? [startColumn + 1] : Array.from({ length: endColumn - startColumn }, (_, index) => startColumn + index + 1);
        const chart: SheetChart = {
          id: globalThis.crypto?.randomUUID?.() ?? `chart-${Date.now()}`,
          title: header(startColumn + 1),
          kind,
          tab,
          categories: rangeReference({ row: startRow + 1, col: startColumn }, { row: endRow, col: startColumn }),
          series: columns.map((column) => ({ name: header(column), range: rangeReference({ row: startRow + 1, col: column }, { row: endRow, col: column }) })),
        };
        chartsRef.current = [...chartsRef.current, chart];
        setCharts(chartsRef.current);
        setChartWorkbook(workbook);
        lastSaved.current = "";
        dirtyRef.current = true;
        setActionError("");
      },
    };
  }, []);

  /** Persist a renamed sheet (Drive rename); revert on empty or failure. */
  function commitName() {
    const trimmed = sheetName.trim();
    if (trimmed === "" ) {
      setSheetName(name);
      return;
    }
    if (trimmed !== name) {
      void client.driveRename(nodeId, trimmed)
        .then(() => onNameChange(trimmed))
        .catch((error: unknown) => {
          setSheetName(name);
          setActionError(strings.driveActionFailed(strings.driveRename, driveErrorReason(error) ?? strings.driveUnknownError));
        });
    }
  }

  /** Export the live workbook as a real `.xlsx` and download it (ADR 0033) — the
   *  round-trip that lets an alo Sheet leave as a genuine Excel file. */
  function downloadXlsx() {
    const api = apiRef.current;
    if (api === null) return;
    const json = snapshotJson(api);
    if (json === "") return;
    const bytes = univerSnapshotToXlsx(
      JSON.parse(json) as Parameters<typeof univerSnapshotToXlsx>[0],
    );
    saveBlob(new Blob([bytes as BlobPart], { type: XLSX_MIME }), `${sheetName.trim() || name}.xlsx`);
  }

  return (
    <div className={styles.overlay}>
      <header className={styles.head}>
        <button type="button" className={styles.back} onClick={close} aria-label={strings.close} title={strings.close}>
          <ChevronLeft size={18} />
        </button>
        <div className={styles.documentIdentity}>
          <input
            ref={nameRef}
            className={styles.nameInput}
            style={{ inlineSize: `${Math.min(Math.max(sheetName.length + 1, 8), 40)}ch` }}
            value={sheetName}
            aria-label={strings.sheetName}
            onChange={(e) => setSheetName(e.target.value)}
            onBlur={commitName}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
              else if (e.key === "Escape") {
                setSheetName(name);
                e.currentTarget.blur();
              }
            }}
          />
          <span className={styles.saved} aria-live="polite">
            {saveState === "saving" ? (
              <>
                <Spinner size={12} /> {strings.docSaving}
              </>
            ) : (
              <>
                <Check size={14} className={styles.savedIcon} /> {strings.sheetSaved}
              </>
            )}
          </span>
        </div>
        <div className={styles.grow} />
        <div className={styles.headActions}>
          <button
            type="button"
            className={styles.export}
            aria-pressed={agentOpen}
            aria-label={strings.recordAgentPanelToggle}
            onClick={() => setAgentOpen((open) => !open)}
            title={strings.recordAgentTitle}
          >
            <Bot size={16} />
            <span>{strings.recordAgentPanelToggle}</span>
          </button>
          <button
            type="button"
            className={styles.export}
            onClick={downloadXlsx}
            disabled={!ready}
            title={strings.sheetDownloadXlsx}
          >
            <Download size={16} />
            <span>{strings.sheetExport}</span>
          </button>
          <Menu
            label={strings.sheetMore}
            icon={<MoreHorizontal size={18} />}
            align="end"
            items={[
            {
              key: "rename",
              label: strings.driveRename,
              icon: <Pencil size={15} />,
              onClick: () => {
                nameRef.current?.focus();
                nameRef.current?.select();
              },
            },
            {
              key: "export",
              label: strings.sheetDownloadXlsx,
              icon: <Download size={15} />,
              onClick: downloadXlsx,
            },
            {
              key: "close",
              label: strings.close,
              icon: <X size={15} />,
              onClick: close,
              divider: true,
            },
            ]}
          />
        </div>
      </header>
      <SheetRibbon actions={actions} disabled={!ready} formulaCategories={FORMULA_CATEGORIES} activeBorder={activeBorder} selectionFormatting={selectionFormatting} />
      <div className={styles.body}>
        {loadError !== null ? (
          <div className={styles.sheetLoadError} role="alert">
            <h2>{strings.sheetLoadFailedTitle}</h2>
            <p>{strings.driveLoadFailed(loadError)}</p>
            <button type="button" onClick={() => setLoadAttempt((value) => value + 1)}>{strings.driveRetry}</button>
          </div>
        ) : !ready ? (
          <SheetSkeleton />
        ) : null}
        {actionError !== "" && (
          <div className={styles.sheetActionError} role="alert">
            <span>{actionError}</span>
            <button type="button" onClick={() => setActionError("")} aria-label={strings.close}><X size={16} /></button>
          </div>
        )}
        <div ref={containerRef} className={styles.univer} />
        {/* One right-hand rail over the grid: the workbook's agent (A8.4) when
            it is open, then its charts. Two floating panels at the same corner
            would sit on top of each other. */}
        {(agentOpen || charts.length > 0) && (
          <div className="absolute right-5 top-5 z-20 flex max-h-[calc(100%-2.5rem)] w-[min(26rem,calc(100%-2.5rem))] flex-col gap-3 overflow-y-auto">
        {agentOpen && (
          <aside className="rounded-2xl border border-subtle bg-surface/95 p-4 shadow-xl backdrop-blur" aria-label={strings.recordAgentTitle}>
            <RecordAgentPanel
              product="sheets"
              recordKind="sheet"
              recordId={nodeId}
              recordLabel={sheetName.trim() === "" ? name : sheetName}
              origin={origin}
              onBeforeNavigate={flushSave}
            />
          </aside>
        )}
        {charts.length > 0 && (
          <aside className="flex flex-col gap-3 rounded-2xl border border-subtle bg-surface/95 p-4 shadow-xl backdrop-blur" aria-label={strings.sheetCharts}>
            <div className="border-b border-subtle pb-3">
              <h2 className="m-0 text-base font-semibold text-primary">{strings.sheetCharts}</h2>
              <p className="mb-0 mt-1 text-xs leading-5 text-tertiary">{strings.sheetChartExcelLimit}</p>
            </div>
            {charts.map((chart) => {
              const result = sheetChartModel(chartWorkbook, chart);
              return <section key={chart.id} className="rounded-xl border border-subtle bg-surface p-3">
                <div className="mb-3 flex items-center justify-between gap-3">
                  <h3 className="m-0 truncate text-sm font-semibold text-primary">{chart.title}</h3>
                  <button type="button" className="flex size-9 shrink-0 items-center justify-center rounded-lg border-0 bg-transparent text-tertiary hover:bg-raised hover:text-primary" aria-label={strings.sheetChartRemove} title={strings.sheetChartRemove} onClick={() => { chartsRef.current = chartsRef.current.filter((item) => item.id !== chart.id); setCharts(chartsRef.current); lastSaved.current = ""; dirtyRef.current = true; }}><Trash2 size={16} /></button>
                </div>
                {"model" in result ? <div className="h-64"><Suspense fallback={<div className="grid h-full place-items-center"><Spinner /></div>}><Chart model={result.model} label={chart.title} /></Suspense></div> : <p className="m-0 rounded-lg bg-raised p-4 text-sm text-tertiary">{strings[result.error]}</p>}
              </section>;
            })}
          </aside>
        )}
          </div>
        )}
      </div>
    </div>
  );
}

function SheetSkeleton() {
  return (
    <div className={styles.sheetSkeleton} role="status" aria-label={strings.sheetLoading} aria-busy="true">
      <span className={styles.sheetSkeletonFormula} />
      <span className={styles.sheetSkeletonGrid} />
    </div>
  );
}

/** The active workbook's snapshot as a stable JSON string, or "" if unavailable. */
function snapshotJson(api: ReturnType<typeof createUniver>["univerAPI"]): string {
  try {
    const workbook = api.getActiveWorkbook();
    if (workbook === null || workbook === undefined) return "";
    return JSON.stringify(workbook.save());
  } catch {
    return "";
  }
}
