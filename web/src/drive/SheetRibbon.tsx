// alo Sheet's own ribbon — our UI over Univer's open engine (ADR 0033). Univer's
// built-in toolbar is hidden (`toolbar: false`); this renders in its place and
// drives the engine through the `SheetActions` the editor passes in. Pure
// presentation: it holds no Univer types, so the engine coupling lives in one
// place (SheetEditor), per one-file-one-reason.
import { useEffect, useRef, useState } from "react";
import {
  AArrowDown,
  AArrowUp,
  AlignCenter,
  AlignLeft,
  AlignRight,
  AlignVerticalJustifyCenter,
  AlignVerticalJustifyEnd,
  AlignVerticalJustifyStart,
  ArrowRightFromLine,
  ArrowDownAZ,
  ArrowDownZA,
  Baseline,
  BarChart3,
  Bold,
  ClipboardPaste,
  CalendarDays,
  Check,
  ChevronDown,
  Database,
  DollarSign,
  Copy,
  Eraser,
  Eye,
  EyeOff,
  Filter,
  Globe2,
  Italic,
  Image,
  Info,
  Link,
  ListChecks,
  ListTree,
  LineChart,
  LockKeyhole,
  Maximize2,
  MessageSquare,
  MoreHorizontal,
  PaintBucket,
  PieChart,
  Palette,
  PanelRightOpen,
  Grid3X3,
  Radical,
  Redo2,
  RotateCcw,
  Rows3,
  Scissors,
  ScissorsLineDashed,
  Search,
  Sigma,
  TextCursorInput,
  Wrench,
  Snowflake,
  Strikethrough,
  StickyNote,
  TableCellsMerge,
  TableCellsSplit,
  TableColumnsSplit,
  TableRowsSplit,
  Table2,
  Tags,
  Columns3,
  UnlockKeyhole,
  Underline,
  Undo2,
  WrapText,
  ZoomIn,
  ZoomOut,
} from "lucide-react";

import { strings } from "../i18n";
import { ColorPicker } from "../ds";
import type { SheetChartKind } from "./sheetDocument";
import styles from "./SheetRibbon.module.css";

/** What the ribbon can ask the engine to do. Implemented by SheetEditor against
 *  Univer's facade; the ribbon itself never touches Univer. */
export interface SheetActions {
  /** Run a Univer command by id (the toggle formats: bold/italic/…). */
  exec: (commandId: string, params?: Record<string, unknown>) => void;
  setFontFamily: (family: string) => void;
  setFontSize: (size: number) => void;
  adjustFontSize: (delta: number) => void;
  setFontColor: (hex: string) => void;
  setFillColor: (hex: string) => void;
  setBorder: (kind: BorderKind) => void;
  setRotation: (rotation: number | "vertical") => void;
  align: (a: "left" | "center" | "right") => void;
  valign: (a: "top" | "middle" | "bottom") => void;
  setWrapMode: (mode: "overflow" | "wrap" | "clip") => void;
  mergeCells: (mode: "all" | "across" | "vertical" | "unmerge") => void;
  setNumberFormat: (pattern: string) => void;
  setRowHeight: (height: number) => void;
  setColumnWidth: (width: number) => void;
  autoFitRow: () => void;
  autoFitColumn: () => void;
  hideRow: () => void;
  showRows: () => void;
  hideColumn: () => void;
  showColumns: () => void;
  toggleGridlines: () => void;
  setGridlineColor: (hex: string) => void;
  setSheetDirection: (direction: "ltr" | "rtl") => void;
  insertRow: (where: "before" | "after") => void;
  insertColumn: (where: "before" | "after") => void;
  deleteRow: () => void;
  deleteColumn: () => void;
  clearContents: () => void;
  clearFormats: () => void;
  freezeAtSelection: () => void;
  freezeTopRow: () => void;
  freezeFirstColumn: () => void;
  unfreeze: () => void;
  splitTextToColumns: () => void;
  protectRange: () => void;
  unprotectRange: () => void;
  protectSheet: () => void;
  unprotectSheet: () => void;
  adjustZoom: (delta: number) => void;
  resetZoom: () => void;
  /** Apply a named cell style (font size + weight + optional colour). */
  stylePreset: (p: { size: number; bold: boolean; color?: string }) => void;
  undo: () => void;
  redo: () => void;
  addChart: (kind: SheetChartKind) => void;
}

export interface FormulaCategory {
  key: string;
  label: string;
  functions: string[];
}

export interface SheetSelectionFormatting {
  activeKeys: string[];
  fontFamily: string | null;
  fontSize: number | null;
  numberPattern: string | null;
  fontColor: string;
  fillColor: string;
}

export type BorderKind = "all" | "outer" | "inside" | "top" | "bottom" | "left" | "right" | "horizontal" | "vertical" | "none" | "diag-down" | "diag-up" | "diag-down-center" | "diag-down-both" | "diag-up-center";

// Univer command ids (verified against @univerjs/sheets and the extra presets
// wired in SheetEditor: sort, filter, find-replace).
const CMD_BOLD = "sheet.command.set-range-bold";
const CMD_ITALIC = "sheet.command.set-range-italic";
const CMD_UNDERLINE = "sheet.command.set-range-underline";
const CMD_STRIKE = "sheet.command.set-range-stroke";
const CMD_COPY = "sheet.command.copy";
const CMD_CUT = "sheet.command.cut";
const CMD_PASTE = "sheet.command.paste";
const CMD_SORT_ASC = "sheet.command.sort-range-asc";
const CMD_SORT_DESC = "sheet.command.sort-range-desc";
const CMD_FILTER = "sheet.command.smart-toggle-filter";
const CMD_DEFINED_NAMES = "sidebar.operation.defined-name";
const CMD_FIND = "ui.operation.open-find-dialog";
const CMD_CONDITIONAL_FORMATTING = "sheet.operation.open";
const CMD_DATA_VALIDATION = "data-validation.operation.toggle-validation-panel";
// Opens Univer's native image picker and inserts the selected file as a
// floating sheet image. Registered by the sheets-drawing UI preset.
const CMD_INSERT_IMAGE = "sheet.command.insert-float-image";
const CMD_HYPERLINK = "sheet.operation.insert-hyper-link-toolbar";
const CMD_NOTE = "sheet.command.toggle-note-popup";
const CMD_TABLE = "sheet.operation.open-table-selector";
const CMD_COMMENT = "sheet.operation.show-comment-modal";
const CMD_COMMENT_PANEL = "sheet.operation.toggle-comment-panel";
const CMD_INSERT_FUNCTION = "formula-ui.operation.insert-function";

// Number-format quick presets (label symbol → Excel pattern).
const NUM_PERCENT = "0.00%";
const NUM_CURRENCY = "€ #,##0.00";
const NUM_COMMA = "#,##0";

// Cell-style presets (the Styles gallery in the mockup).
const STYLE_PRESETS: { key: string; label: string; size: number; bold: boolean; color?: string }[] = [
  { key: "normal", label: strings.sheetStyleDefault, size: 11, bold: false, color: "#111827" },
  { key: "h1", label: strings.sheetStyleHeading1, size: 20, bold: true, color: "#111827" },
  { key: "h2", label: strings.sheetStyleHeading2, size: 16, bold: true, color: "#111827" },
  { key: "h3", label: strings.sheetStyleHeading3, size: 14, bold: true, color: "#374151" },
  { key: "h4", label: strings.sheetStyleHeading4, size: 12, bold: true, color: "#374151" },
  { key: "title", label: strings.sheetStyleTitle, size: 28, bold: true, color: "#111827" },
  { key: "subtitle", label: strings.sheetStyleSubtitle, size: 15, bold: false, color: "#6b7280" },
];
const STYLE_GLYPHS: Record<string, string> = {
  normal: "Aa",
  h1: "H1",
  h2: "H2",
  h3: "H3",
  h4: "H4",
  title: "T",
  subtitle: "ST",
};

const FONTS = ["Calibri", "Arial", "Times New Roman", "Georgia", "Verdana", "Courier New"];
const SIZES = [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72];

// Number-format presets (EU-friendly). Value is an Excel-style format pattern.
const NUMBER_FORMATS: { key: string; label: string; preview: string; pattern: string }[] = [
  { key: "general", label: strings.sheetFormatGeneral, preview: strings.sheetFormatPreviewGeneral, pattern: "General" },
  { key: "number", label: strings.sheetFormatNumber, preview: strings.sheetFormatPreviewNumber, pattern: "#,##0.00" },
  { key: "currency", label: strings.sheetFormatCurrency, preview: strings.sheetFormatPreviewCurrency, pattern: "€ #,##0.00" },
  { key: "percent", label: strings.sheetFormatPercentage, preview: strings.sheetFormatPreviewPercentage, pattern: "0.00%" },
  { key: "date", label: strings.sheetFormatDate, preview: strings.sheetFormatPreviewDate, pattern: "yyyy-mm-dd" },
  { key: "text", label: strings.sheetFormatText, preview: strings.sheetFormatPreviewText, pattern: "@" },
];

const BORDER_GLYPHS: Record<string, string> = {
  all: "⊞",
  outer: "□",
  inside: "+",
  top: "▔",
  bottom: "▁",
  left: "▏",
  right: "▕",
  horizontal: "☰",
  vertical: "Ⅲ",
  none: "⊘",
  "diag-down": "╲",
  "diag-up": "╱",
  "diag-down-center": "╲┼",
  "diag-down-both": "╲╳",
  "diag-up-center": "╱┼",
};

const PRIMARY_TABS = ["home", "formulas", "others"] as const;
type PrimaryTab = (typeof PRIMARY_TABS)[number];
// Tabs whose tools need Univer plugins we haven't wired yet — honest placeholder.

function primaryTabLabel(tab: PrimaryTab): string {
  switch (tab) {
    case "home":
      return strings.sheetTabHome;
    case "formulas":
      return strings.sheetTabFormulas;
    case "others":
      return strings.sheetTabOthers;
  }
}

export function SheetRibbon({ actions, disabled, formulaCategories, activeBorder, selectionFormatting }: { actions: SheetActions; disabled: boolean; formulaCategories: FormulaCategory[]; activeBorder: BorderKind | null; selectionFormatting: SheetSelectionFormatting }) {
  const [tab, setTab] = useState<PrimaryTab>("home");
  const activeToolRef = useRef<HTMLElement | null>(null);
  const ribbonRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const activeClass = styles.toolActive;
    if (activeClass === undefined) return;
    ribbonRef.current?.querySelectorAll<HTMLElement>("[data-format-key]").forEach((tool) => tool.classList.remove(activeClass));
    activeToolRef.current?.classList.remove(activeClass);
    activeToolRef.current = null;
    selectionFormatting.activeKeys.forEach((key) => {
      ribbonRef.current?.querySelectorAll<HTMLElement>(`[data-format-key="${key}"]`).forEach((tool) => tool.classList.add(activeClass));
    });
    if (activeBorder === null) return;
    const borderTool = ribbonRef.current?.querySelector<HTMLElement>(`[data-border-kind="${activeBorder}"]`);
    if (borderTool === null || borderTool === undefined) return;
    borderTool.classList.add(activeClass);
    activeToolRef.current = borderTool;
  }, [activeBorder, selectionFormatting]);

  useEffect(() => {
    const closeMenus = (except?: HTMLDetailsElement) => {
      ribbonRef.current?.querySelectorAll<HTMLDetailsElement>("details[data-ribbon-menu][open]").forEach((menu) => {
        if (menu !== except) menu.removeAttribute("open");
      });
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      const openMenu = target instanceof Element ? target.closest<HTMLDetailsElement>("details[data-ribbon-menu][open]") : null;
      closeMenus(openMenu ?? undefined);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeMenus();
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  const markActiveTool = (event: React.MouseEvent<HTMLDivElement>) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const groups = target.closest(`.${styles.groups}`);
    if (groups === null) return;
    const clicked = target.closest<HTMLElement>("button, summary, label");
    if (clicked === null) return;
    const menu = clicked.closest<HTMLDetailsElement>("details[data-ribbon-menu]");
    const tool = menu?.querySelector<HTMLElement>(":scope > summary") ?? clicked;
    const activeClass = styles.toolActive;
    const disabledClass = styles.brandedSelectDisabled;
    if (activeClass === undefined || disabledClass === undefined) return;
    if (tool.matches(":disabled") || tool.classList.contains(disabledClass)) return;
    activeToolRef.current?.classList.remove(activeClass);
    tool.classList.add(activeClass);
    activeToolRef.current = tool;
  };

  return (
    <div ref={ribbonRef} className={styles.ribbon} role="toolbar" aria-label={strings.sheetRibbon} onClickCapture={markActiveTool}>
      <div className={styles.tabs} role="tablist">
        {PRIMARY_TABS.map((t) => (
          <button
            key={t}
            type="button"
            role="tab"
            aria-selected={t === tab}
            className={t === tab ? styles.tabActive : styles.tab}
            onClick={() => setTab(t)}
          >
            {primaryTabLabel(t)}
          </button>
        ))}
      </div>

      {tab === "home" && <HomeTab actions={actions} disabled={disabled} selectionFormatting={selectionFormatting} />}
      {tab === "formulas" && <FormulasTab actions={actions} disabled={disabled} categories={formulaCategories} />}
      {tab === "others" && (
        <div className={styles.othersGroups} aria-label={strings.sheetTabOthers}>
          <PageLayoutTab actions={actions} disabled={disabled} />
          <DataTab actions={actions} disabled={disabled} />
          <ReviewTab actions={actions} disabled={disabled} />
          <ViewTab actions={actions} disabled={disabled} />
        </div>
      )}
    </div>
  );
}

function FormulaCategoryIcon({ category }: { category: string }) {
  switch (category) {
    case "financial": return <DollarSign size={18} />;
    case "date": return <CalendarDays size={18} />;
    case "math": return <Radical size={18} />;
    case "lookup": return <Search size={18} />;
    case "database": return <Database size={18} />;
    case "text": return <TextCursorInput size={18} />;
    case "information": return <Info size={18} />;
    case "engineering": return <Wrench size={18} />;
    case "web": return <Globe2 size={18} />;
    case "array": return <ListTree size={18} />;
    default: return <Sigma size={18} />;
  }
}

function FormulasTab({ actions, disabled, categories = [] }: { actions: SheetActions; disabled: boolean; categories?: FormulaCategory[] }) {
  const insert = (value: string) => actions.exec(CMD_INSERT_FUNCTION, { value });
  return (
    <div className={styles.groups}>
      <Group label={strings.sheetGroupFunctionLibrary}>
        <div className={`${styles.row} ${styles.formulaLibrary}`}>
          <IconBtn label={strings.sheetAutoSum} onClick={() => insert("SUM")} disabled={disabled} showLabel formatKey="formula-SUM">
            <Sigma size={18} />
          </IconBtn>
          <CommandBtn label={strings.sheetAverage} onClick={() => insert("AVERAGE")} disabled={disabled} formatKey="formula-AVERAGE"><span>fx</span></CommandBtn>
          <CommandBtn label={strings.sheetCount} onClick={() => insert("COUNT")} disabled={disabled} formatKey="formula-COUNT"><span>fx</span></CommandBtn>
          <CommandBtn label={strings.sheetMinimum} onClick={() => insert("MIN")} disabled={disabled} formatKey="formula-MIN"><span>fx</span></CommandBtn>
          <CommandBtn label={strings.sheetMaximum} onClick={() => insert("MAX")} disabled={disabled} formatKey="formula-MAX"><span>fx</span></CommandBtn>
        </div>
      </Group>
      <Group label={strings.sheetGroupFunctionCategories}>
        <div className={styles.formulaCategories}>
          {categories.map((category) => (
            <RibbonMenu key={category.key} label={category.label} icon={<FormulaCategoryIcon category={category.key} />} disabled={disabled} variant="wide" formatKey={`formula-category-${category.key}`}>
              <div className={styles.formulaFunctionList}>
                {category.functions.map((name) => <TextBtn key={name} label={name} onClick={() => insert(name)} disabled={disabled} />)}
              </div>
            </RibbonMenu>
          ))}
        </div>
      </Group>
    </div>
  );
}

function HomeTab({ actions, disabled, selectionFormatting }: { actions: SheetActions; disabled: boolean; selectionFormatting: SheetSelectionFormatting }) {
  return (
    <div className={styles.groups}>
      <Group label={strings.sheetGroupHistory}>
        <div className={styles.row}>
          <IconBtn label={strings.sheetUndo} onClick={actions.undo} disabled={disabled} showLabel>
            <Undo2 size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetRedo} onClick={actions.redo} disabled={disabled} showLabel>
            <Redo2 size={16} />
          </IconBtn>
        </div>
      </Group>

      <Group label={strings.sheetGroupClipboard}>
        <div className={styles.row}>
          <IconBtn label={strings.sheetPaste} onClick={() => actions.exec(CMD_PASTE)} disabled={disabled} large showLabel>
            <ClipboardPaste size={20} />
          </IconBtn>
          <div className={styles.commandStack}>
            <CommandBtn label={strings.sheetCut} onClick={() => actions.exec(CMD_CUT)} disabled={disabled}>
              <Scissors size={16} />
            </CommandBtn>
            <CommandBtn label={strings.sheetCopy} onClick={() => actions.exec(CMD_COPY)} disabled={disabled}>
              <Copy size={16} />
            </CommandBtn>
          </div>
        </div>
      </Group>

      <Group label={strings.sheetGroupFont}>
        <div className={styles.rowStack}>
          <div className={styles.row}>
            <BrandedSelect className={styles.fontSelect} label={strings.sheetFontFamily} disabled={disabled} defaultValue="Calibri" selectedValue={selectionFormatting.fontFamily} options={FONTS.map((font) => ({ value: font, label: font }))} onSelect={actions.setFontFamily} />
            <BrandedSelect className={styles.sizeSelect} label={strings.sheetFontSize} disabled={disabled} defaultValue="11" selectedValue={selectionFormatting.fontSize === null ? null : String(selectionFormatting.fontSize)} options={SIZES.map((size) => ({ value: String(size), label: String(size) }))} onSelect={(value) => actions.setFontSize(Number(value))} />
            <IconBtn label={strings.sheetFontGrow} onClick={() => actions.adjustFontSize(1)} disabled={disabled}>
              <AArrowUp size={16} />
            </IconBtn>
            <IconBtn label={strings.sheetFontShrink} onClick={() => actions.adjustFontSize(-1)} disabled={disabled}>
              <AArrowDown size={16} />
            </IconBtn>
          </div>
          <div className={styles.row}>
            <IconBtn label={strings.sheetBold} onClick={() => actions.exec(CMD_BOLD)} disabled={disabled} formatKey="bold">
              <Bold size={16} />
            </IconBtn>
            <IconBtn label={strings.sheetItalic} onClick={() => actions.exec(CMD_ITALIC)} disabled={disabled} formatKey="italic">
              <Italic size={16} />
            </IconBtn>
            <IconBtn label={strings.sheetUnderline} onClick={() => actions.exec(CMD_UNDERLINE)} disabled={disabled} formatKey="underline">
              <Underline size={16} />
            </IconBtn>
            <IconBtn label={strings.sheetStrike} onClick={() => actions.exec(CMD_STRIKE)} disabled={disabled} formatKey="strike">
              <Strikethrough size={16} />
            </IconBtn>
            <ColorBtn label={strings.sheetFontColor} onPick={actions.setFontColor} disabled={disabled} formatKey="font-color" selectedColor={selectionFormatting.fontColor}>
              <Baseline size={16} />
            </ColorBtn>
            <ColorBtn label={strings.sheetFillColor} onPick={actions.setFillColor} disabled={disabled} formatKey="fill-color" selectedColor={selectionFormatting.fillColor}>
              <PaintBucket size={16} />
            </ColorBtn>
          </div>
        </div>
      </Group>

      <Group label={strings.sheetGroupBorders}>
        <div className={styles.borderControls}>
          <BorderBtn kind="all" label={strings.sheetBordersAll} onClick={() => actions.setBorder("all")} disabled={disabled} />
          <BorderBtn kind="outer" label={strings.sheetBordersOuter} onClick={() => actions.setBorder("outer")} disabled={disabled} />
          <BorderBtn kind="inside" label={strings.sheetBordersInside} onClick={() => actions.setBorder("inside")} disabled={disabled} />
          <BorderBtn kind="top" label={strings.sheetBordersTop} onClick={() => actions.setBorder("top")} disabled={disabled} />
          <BorderBtn kind="bottom" label={strings.sheetBordersBottom} onClick={() => actions.setBorder("bottom")} disabled={disabled} />
          <BorderBtn kind="left" label={strings.sheetBordersLeft} onClick={() => actions.setBorder("left")} disabled={disabled} />
          <BorderBtn kind="right" label={strings.sheetBordersRight} onClick={() => actions.setBorder("right")} disabled={disabled} />
          <BorderBtn kind="horizontal" label={strings.sheetBordersHorizontal} onClick={() => actions.setBorder("horizontal")} disabled={disabled} />
          <BorderBtn kind="vertical" label={strings.sheetBordersVertical} onClick={() => actions.setBorder("vertical")} disabled={disabled} />
          <BorderBtn kind="none" label={strings.sheetBordersNone} onClick={() => actions.setBorder("none")} disabled={disabled} />
          <AdvancedBorderMenu actions={actions} disabled={disabled} />
        </div>
      </Group>

      <Group label={strings.sheetGroupAlignment}>
        <div className={styles.rowStack}>
          <div className={styles.row}>
            <IconBtn label={strings.sheetAlignTop} onClick={() => actions.valign("top")} disabled={disabled} formatKey="valign-top">
              <AlignVerticalJustifyStart size={16} />
            </IconBtn>
            <IconBtn label={strings.sheetAlignMiddle} onClick={() => actions.valign("middle")} disabled={disabled} formatKey="valign-middle">
              <AlignVerticalJustifyCenter size={16} />
            </IconBtn>
            <IconBtn label={strings.sheetAlignBottom} onClick={() => actions.valign("bottom")} disabled={disabled} formatKey="valign-bottom">
              <AlignVerticalJustifyEnd size={16} />
            </IconBtn>
          </div>
          <div className={styles.row}>
            <IconBtn label={strings.sheetAlignLeft} onClick={() => actions.align("left")} disabled={disabled} formatKey="align-left">
              <AlignLeft size={16} />
            </IconBtn>
            <IconBtn label={strings.sheetAlignCenter} onClick={() => actions.align("center")} disabled={disabled} formatKey="align-center">
              <AlignCenter size={16} />
            </IconBtn>
            <IconBtn label={strings.sheetAlignRight} onClick={() => actions.align("right")} disabled={disabled} formatKey="align-right">
              <AlignRight size={16} />
            </IconBtn>
          </div>
        </div>
      </Group>

      <Group label={strings.sheetGroupWrap}>
        <div className={styles.wrapControls}>
          <IconBtn label={strings.sheetWrapOverflow} onClick={() => actions.setWrapMode("overflow")} disabled={disabled} formatKey="wrap-overflow">
            <ArrowRightFromLine size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetWrapText} onClick={() => actions.setWrapMode("wrap")} disabled={disabled} formatKey="wrap-wrap">
            <WrapText size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetWrapClip} onClick={() => actions.setWrapMode("clip")} disabled={disabled} formatKey="wrap-clip">
            <ScissorsLineDashed size={16} />
          </IconBtn>
        </div>
      </Group>

      <Group label={strings.sheetGroupMerge}>
        <div className={styles.mergeControls}>
          <IconBtn label={strings.sheetMergeAll} onClick={() => actions.mergeCells("all")} disabled={disabled} formatKey="merge-all">
            <TableCellsMerge size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetMergeAcross} onClick={() => actions.mergeCells("across")} disabled={disabled}>
            <TableRowsSplit size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetMergeVertically} onClick={() => actions.mergeCells("vertical")} disabled={disabled}>
            <TableColumnsSplit size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetUnmerge} onClick={() => actions.mergeCells("unmerge")} disabled={disabled}>
            <TableCellsSplit size={16} />
          </IconBtn>
        </div>
      </Group>

      <Group label={strings.sheetGroupRotation}>
        <div className={styles.rotationControls}>
          <IconBtn label={strings.sheetRotationNone} onClick={() => actions.setRotation(0)} disabled={disabled} formatKey="rotation-0">
            <RotateCcw size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetRotation45} onClick={() => actions.setRotation(45)} disabled={disabled} formatKey="rotation-45">
            <span className={styles.rotationGlyph}>45°</span>
          </IconBtn>
          <IconBtn label={strings.sheetRotationMinus45} onClick={() => actions.setRotation(-45)} disabled={disabled} formatKey="rotation--45">
            <span className={styles.rotationGlyph}>−45°</span>
          </IconBtn>
          <IconBtn label={strings.sheetRotation90} onClick={() => actions.setRotation(90)} disabled={disabled} formatKey="rotation-90">
            <span className={styles.rotationGlyph}>90°</span>
          </IconBtn>
          <IconBtn label={strings.sheetRotationMinus90} onClick={() => actions.setRotation(-90)} disabled={disabled} formatKey="rotation--90">
            <span className={styles.rotationGlyph}>−90°</span>
          </IconBtn>
          <IconBtn label={strings.sheetRotationVertical} onClick={() => actions.setRotation("vertical")} disabled={disabled} formatKey="rotation-vertical">
            <Baseline size={16} className={styles.verticalTextIcon} />
          </IconBtn>
        </div>
      </Group>

      <Group label={strings.sheetGroupNumber}>
        <div className={styles.rowStack}>
          <BrandedSelect className={styles.numberSelect} label={strings.sheetNumberFormat} disabled={disabled} defaultValue="general" selectedValue={NUMBER_FORMATS.find((format) => format.pattern === selectionFormatting.numberPattern)?.key ?? "general"} options={NUMBER_FORMATS.map((format) => ({ value: format.key, label: format.label, preview: format.preview }))} onSelect={(value) => {
            const format = NUMBER_FORMATS.find((candidate) => candidate.key === value);
            if (format) actions.setNumberFormat(format.pattern);
          }} />
          <div className={styles.numberQuickActions}>
            <NumberBtn label={strings.sheetPercent} onClick={() => actions.setNumberFormat(NUM_PERCENT)} disabled={disabled} formatKey="number-percent">
              %
            </NumberBtn>
            <NumberBtn label={strings.sheetCurrency} onClick={() => actions.setNumberFormat(NUM_CURRENCY)} disabled={disabled} formatKey="number-currency">
              €
            </NumberBtn>
            <NumberBtn label={strings.sheetComma} onClick={() => actions.setNumberFormat(NUM_COMMA)} disabled={disabled} compact formatKey="number-comma">
              1,000
            </NumberBtn>
          </div>
        </div>
      </Group>

      <Group label={strings.sheetGroupStyles}>
        <StyleGallery actions={actions} disabled={disabled} />
      </Group>
      <Group label={strings.sheetGroupCharts}>
        <div className={styles.toolStrip}>
          <IconBtn label={strings.sheetChartBar} onClick={() => actions.addChart("bar")} disabled={disabled}><BarChart3 size={16} /></IconBtn>
          <IconBtn label={strings.sheetChartLine} onClick={() => actions.addChart("line")} disabled={disabled}><LineChart size={16} /></IconBtn>
          <IconBtn label={strings.sheetChartPie} onClick={() => actions.addChart("pie")} disabled={disabled}><PieChart size={16} /></IconBtn>
        </div>
      </Group>

      <Group label={strings.sheetGroupCells}>
        <div className={styles.cellControls}>
          <IconBtn label={strings.sheetInsertRowAbove} onClick={() => actions.insertRow("before")} disabled={disabled}>
            <CellActionGlyph action="add"><Rows3 size={16} /></CellActionGlyph>
          </IconBtn>
          <IconBtn label={strings.sheetInsertColLeft} onClick={() => actions.insertColumn("before")} disabled={disabled}>
            <CellActionGlyph action="add"><Columns3 size={16} /></CellActionGlyph>
          </IconBtn>
          <IconBtn label={strings.sheetDeleteRow} onClick={actions.deleteRow} disabled={disabled}>
            <CellActionGlyph action="remove"><Rows3 size={16} /></CellActionGlyph>
          </IconBtn>
          <IconBtn label={strings.sheetDeleteColumn} onClick={actions.deleteColumn} disabled={disabled}>
            <CellActionGlyph action="remove"><Columns3 size={16} /></CellActionGlyph>
          </IconBtn>
          <IconBtn label={strings.sheetClearFormats} onClick={actions.clearFormats} disabled={disabled}>
            <Eraser size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetClearContents} onClick={actions.clearContents} disabled={disabled}>
            <ScissorsLineDashed size={16} />
          </IconBtn>
          <details className={styles.cellMore} data-ribbon-menu onToggle={handleRibbonMenuToggle}>
            <summary className={disabled ? styles.cellMoreDisabled : undefined} aria-label={strings.sheetMoreCellOptions} title={strings.sheetMoreCellOptions}>
              <MoreHorizontal size={16} />
            </summary>
            <div className={styles.ribbonMenuPanel} data-ribbon-floating-panel>
            <TextBtn label={strings.sheetInsertRowBelow} onClick={() => actions.insertRow("after")} disabled={disabled} />
            <TextBtn label={strings.sheetInsertColRight} onClick={() => actions.insertColumn("after")} disabled={disabled} />
            </div>
          </details>
        </div>
      </Group>

      <Group label={strings.sheetGroupEditing}>
        <EditingControls actions={actions} disabled={disabled} />
      </Group>

      <Group label={strings.sheetInsert}>
        <div className={styles.row}>
          <IconBtn label={strings.sheetInsertTable} onClick={() => actions.exec(CMD_TABLE)} disabled={disabled} showLabel>
            <Table2 size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetInsertLink} onClick={() => actions.exec(CMD_HYPERLINK)} disabled={disabled} showLabel>
            <Link size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetInsertImage} onClick={() => actions.exec(CMD_INSERT_IMAGE)} disabled={disabled} showLabel>
            <Image size={16} />
          </IconBtn>
        </div>
      </Group>
    </div>
  );
}

/** Sort / Filter / Find — shared by Home's Editing group and the Data tab
 *  (powered by the sort, filter, and find-replace presets). */
function EditingControls({ actions, disabled }: { actions: SheetActions; disabled: boolean }) {
  return (
    <div className={styles.editingGrid}>
      <IconBtn label={strings.sheetSortAsc} onClick={() => actions.exec(CMD_SORT_ASC)} disabled={disabled}>
        <ArrowDownAZ size={16} />
      </IconBtn>
      <IconBtn label={strings.sheetSortDesc} onClick={() => actions.exec(CMD_SORT_DESC)} disabled={disabled}>
        <ArrowDownZA size={16} />
      </IconBtn>
      <IconBtn label={strings.sheetFilter} onClick={() => actions.exec(CMD_FILTER)} disabled={disabled}>
        <Filter size={16} />
      </IconBtn>
      <IconBtn label={strings.sheetFindReplace} onClick={() => actions.exec(CMD_FIND)} disabled={disabled}>
        <Search size={16} />
      </IconBtn>
    </div>
  );
}

function DataTab({ actions, disabled }: { actions: SheetActions; disabled: boolean }) {
  return (
    <div className={styles.groups}>
      <Group label={strings.sheetGroupSortFilter}>
        <EditingControls actions={actions} disabled={disabled} />
      </Group>
      <Group label={strings.sheetGroupDataTools}>
        <div className={styles.toolStrip}>
          <IconBtn label={strings.sheetDataValidation} onClick={() => actions.exec(CMD_DATA_VALIDATION)} disabled={disabled} formatKey="data-validation">
            <ListChecks size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetTextToColumns} onClick={actions.splitTextToColumns} disabled={disabled}>
            <Columns3 size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetNamedRanges} onClick={() => actions.exec(CMD_DEFINED_NAMES, { value: "open" })} disabled={disabled}>
            <Tags size={16} />
          </IconBtn>
        </div>
      </Group>
      <Group label={strings.sheetGroupStyles}>
        <IconBtn label={strings.sheetConditionalFormatting} onClick={() => actions.exec(CMD_CONDITIONAL_FORMATTING)} disabled={disabled} formatKey="conditional-formatting">
          <Palette size={16} />
        </IconBtn>
      </Group>
    </div>
  );
}

function ReviewTab({ actions, disabled }: { actions: SheetActions; disabled: boolean }) {
  return (
    <div className={styles.groups}>
      <Group label={strings.sheetGroupNotes}>
        <IconBtn label={strings.sheetNote} onClick={() => actions.exec(CMD_NOTE)} disabled={disabled} formatKey="note">
          <StickyNote size={16} />
        </IconBtn>
      </Group>
      <Group label={strings.sheetGroupComments}>
        <div className={styles.row}>
          <IconBtn label={strings.sheetAddComment} onClick={() => actions.exec(CMD_COMMENT)} disabled={disabled} formatKey="comment">
            <MessageSquare size={16} />
          </IconBtn>
          <IconBtn label={strings.sheetCommentsPanel} onClick={() => actions.exec(CMD_COMMENT_PANEL)} disabled={disabled}>
            <PanelRightOpen size={16} />
          </IconBtn>
        </div>
      </Group>
      <Group label={strings.sheetGroupProtection}>
        <div className={styles.layoutGrid}>
          <IconBtn label={strings.sheetProtectRange} onClick={actions.protectRange} disabled={disabled} formatKey="protected-range"><LockKeyhole size={16} /></IconBtn>
          <IconBtn label={strings.sheetUnprotectRange} onClick={actions.unprotectRange} disabled={disabled}><UnlockKeyhole size={16} /></IconBtn>
          <IconBtn label={strings.sheetProtectSheet} onClick={actions.protectSheet} disabled={disabled} formatKey="protected-sheet"><LockKeyhole size={16} /></IconBtn>
          <IconBtn label={strings.sheetUnprotectSheet} onClick={actions.unprotectSheet} disabled={disabled}><UnlockKeyhole size={16} /></IconBtn>
        </div>
      </Group>
    </div>
  );
}

function PageLayoutTab({ actions, disabled }: { actions: SheetActions; disabled: boolean }) {
  return (
    <div className={styles.groups}>
      <Group label={strings.sheetGroupCellSize}>
        <div className={styles.rowStack}>
          <SizeControl label={strings.sheetRowHeight} autoLabel={strings.sheetAutoFitRow} defaultValue={20} onCommit={actions.setRowHeight} onAutoFit={actions.autoFitRow} disabled={disabled}><Rows3 size={16} /></SizeControl>
          <SizeControl label={strings.sheetColumnWidth} autoLabel={strings.sheetAutoFitColumn} defaultValue={100} onCommit={actions.setColumnWidth} onAutoFit={actions.autoFitColumn} disabled={disabled}><Columns3 size={16} /></SizeControl>
        </div>
      </Group>
      <Group label={strings.sheetGroupVisibility}>
        <div className={styles.layoutGrid}>
          <IconBtn label={strings.sheetHideRow} onClick={actions.hideRow} disabled={disabled} formatKey="hidden-row"><EyeOff size={16} /></IconBtn>
          <IconBtn label={strings.sheetShowRows} onClick={actions.showRows} disabled={disabled}><Eye size={16} /></IconBtn>
          <IconBtn label={strings.sheetHideColumn} onClick={actions.hideColumn} disabled={disabled} formatKey="hidden-column"><EyeOff size={16} /></IconBtn>
          <IconBtn label={strings.sheetShowColumns} onClick={actions.showColumns} disabled={disabled}><Eye size={16} /></IconBtn>
        </div>
      </Group>
      <Group label={strings.sheetGroupSheetOptions}>
        <div className={styles.row}>
          <IconBtn label={strings.sheetToggleGridlines} onClick={actions.toggleGridlines} disabled={disabled} formatKey="gridlines"><Grid3X3 size={16} /></IconBtn>
          <ColorBtn label={strings.sheetGridlineColor} onPick={actions.setGridlineColor} disabled={disabled}><PaintBucket size={16} /></ColorBtn>
        </div>
      </Group>
      <Group label={strings.sheetGroupDirection}>
        <div className={styles.row}>
          <IconBtn label={strings.sheetLeftToRight} onClick={() => actions.setSheetDirection("ltr")} disabled={disabled} formatKey="direction-ltr"><AlignLeft size={16} /></IconBtn>
          <IconBtn label={strings.sheetRightToLeft} onClick={() => actions.setSheetDirection("rtl")} disabled={disabled} formatKey="direction-rtl"><AlignRight size={16} /></IconBtn>
        </div>
      </Group>
    </div>
  );
}

function ViewTab({ actions, disabled }: { actions: SheetActions; disabled: boolean }) {
  return (
    <div className={styles.groups}>
      <Group label={strings.sheetGroupFreeze}>
        <div className={styles.layoutGrid}>
          <IconBtn label={strings.sheetFreezeTopRow} onClick={actions.freezeTopRow} disabled={disabled} formatKey="freeze-top-row"><Rows3 size={16} /></IconBtn>
          <IconBtn label={strings.sheetFreezeFirstColumn} onClick={actions.freezeFirstColumn} disabled={disabled} formatKey="freeze-first-column"><Columns3 size={16} /></IconBtn>
          <IconBtn label={strings.sheetFreeze} onClick={actions.freezeAtSelection} disabled={disabled} formatKey="freeze-panes"><Snowflake size={16} /></IconBtn>
          <IconBtn label={strings.sheetUnfreeze} onClick={actions.unfreeze} disabled={disabled}><UnlockKeyhole size={16} /></IconBtn>
        </div>
      </Group>
      <Group label={strings.sheetGroupZoom}>
        <div className={styles.row}>
          <IconBtn label={strings.sheetZoomOut} onClick={() => actions.adjustZoom(-0.1)} disabled={disabled}><ZoomOut size={16} /></IconBtn>
          <TextBtn label={strings.sheetZoomReset} onClick={actions.resetZoom} disabled={disabled} formatKey="zoom-100" />
          <IconBtn label={strings.sheetZoomIn} onClick={() => actions.adjustZoom(0.1)} disabled={disabled}><ZoomIn size={16} /></IconBtn>
        </div>
      </Group>
    </div>
  );
}

function Group({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className={styles.group}>
      <div className={styles.groupBody}>{children}</div>
      <span className={styles.groupLabel}>{label}</span>
    </div>
  );
}

function IconBtn({
  label,
  onClick,
  disabled,
  children,
  formatKey,
  large = false,
  showLabel = false,
}: {
  label: string;
  onClick: () => void;
  disabled: boolean;
  children: React.ReactNode;
  large?: boolean;
  showLabel?: boolean;
  formatKey?: string;
}) {
  return (
    <button
      type="button"
      className={`${styles.iconBtn} ${large ? styles.iconBtnLarge : ""}`}
      data-format-key={formatKey}
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      title={label}
    >
      {children}
      {showLabel && <span className={styles.iconBtnLabel}>{label}</span>}
    </button>
  );
}

function SizeControl({ label, autoLabel, defaultValue, onCommit, onAutoFit, disabled, children }: { label: string; autoLabel: string; defaultValue: number; onCommit: (value: number) => void; onAutoFit: () => void; disabled: boolean; children: React.ReactNode }) {
  const commit = (input: HTMLInputElement) => {
    const value = Number(input.value);
    if (Number.isFinite(value) && value >= 8 && value <= 500) onCommit(value);
  };
  return (
    <div className={styles.sizeControl} title={label}>
      {children}
      <span>{label}</span>
      <input
        type="number"
        min={8}
        max={500}
        defaultValue={defaultValue}
        disabled={disabled}
        aria-label={label}
        onBlur={(event) => commit(event.currentTarget)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            commit(event.currentTarget);
            event.currentTarget.blur();
          }
        }}
      />
      <span className={styles.sizeUnit}>px</span>
      <button type="button" className={styles.sizeAuto} onClick={onAutoFit} disabled={disabled} aria-label={autoLabel} title={autoLabel}><Maximize2 size={14} /></button>
    </div>
  );
}

function CellActionGlyph({ action, children }: { action: "add" | "remove"; children: React.ReactNode }) {
  return <span className={styles.cellActionGlyph}>{children}<span aria-hidden="true">{action === "add" ? "+" : "−"}</span></span>;
}

function TextBtn({ label, onClick, disabled, formatKey }: { label: string; onClick: () => void; disabled: boolean; formatKey?: string }) {
  return (
    <button
      type="button"
      className={styles.textBtn}
      data-format-key={formatKey}
      onClick={(event) => {
        onClick();
        event.currentTarget.closest("details")?.removeAttribute("open");
      }}
      disabled={disabled}
      title={label}
    >
      {label}
    </button>
  );
}

function CommandBtn({ label, onClick, disabled, children, formatKey }: { label: string; onClick: () => void; disabled: boolean; children: React.ReactNode; formatKey?: string }) {
  return <button type="button" className={styles.commandBtn} data-format-key={formatKey} onClick={onClick} disabled={disabled}>{children}<span>{label}</span></button>;
}

function NumberBtn({ label, onClick, disabled, children, compact = false, formatKey }: { label: string; onClick: () => void; disabled: boolean; children: React.ReactNode; compact?: boolean; formatKey?: string }) {
  return <button type="button" className={`${styles.numberBtn} ${compact ? styles.numberBtnCompact : ""}`} data-format-key={formatKey} onClick={onClick} disabled={disabled} aria-label={label} title={label}>{children}</button>;
}

function BorderBtn({ kind, label, onClick, disabled }: { kind: string; label: string; onClick: () => void; disabled: boolean }) {
  return <button type="button" className={styles.borderBtn} data-border-kind={kind} onClick={onClick} disabled={disabled} aria-label={label} title={label}><span className={styles.borderGlyph} data-kind={kind}>{BORDER_GLYPHS[kind]}</span></button>;
}

function StyleGallery({ actions, disabled }: { actions: SheetActions; disabled: boolean }) {
  const featuredKeys = new Set(["normal", "h1", "title", "subtitle"]);
  const apply = (preset: (typeof STYLE_PRESETS)[number]) => {
    actions.stylePreset({ size: preset.size, bold: preset.bold, ...(preset.color ? { color: preset.color } : {}) });
  };
  const button = (preset: (typeof STYLE_PRESETS)[number], closeMenu = false) => (
    <button
      key={preset.key}
      type="button"
      className={styles.styleBtn}
      data-style-key={preset.key}
      data-format-key={`style-${preset.key}`}
      disabled={disabled}
      onMouseDown={(event) => event.preventDefault()}
      onClick={(event) => {
        apply(preset);
        if (closeMenu) event.currentTarget.closest("details")?.removeAttribute("open");
      }}
      title={preset.label}
      aria-label={preset.label}
    >
      <span className={styles.styleGlyph}>{STYLE_GLYPHS[preset.key]}</span>
    </button>
  );

  return (
    <div className={styles.styleCompactGrid}>
      <div className={styles.styleFeatured}>
        {STYLE_PRESETS.filter((preset) => featuredKeys.has(preset.key)).map((preset) => button(preset))}
      </div>
      <details className={styles.styleMore} data-ribbon-menu onToggle={handleRibbonMenuToggle}>
        <summary className={disabled ? styles.styleMoreDisabled : undefined} aria-label={strings.sheetMoreStyles} title={strings.sheetMoreStyles}>
          <ChevronDown size={16} />
        </summary>
        <div className={styles.styleMorePanel} data-ribbon-floating-panel>
          <div className={styles.styleMoreHeading}>{strings.sheetCellStyles}</div>
          <div className={styles.styleMoreGrid}>
            {STYLE_PRESETS.map((preset) => button(preset, true))}
          </div>
        </div>
      </details>
    </div>
  );
}

function AdvancedBorderMenu({ actions, disabled }: { actions: SheetActions; disabled: boolean }) {
  const [open, setOpen] = useState(false);
  const apply = (kind: BorderKind) => {
    actions.setBorder(kind);
    setOpen(false);
  };
  return (
    <div className={styles.advancedBorderMenu} onBlur={(event) => {
      if (!(event.relatedTarget instanceof Node) || !event.currentTarget.contains(event.relatedTarget)) setOpen(false);
    }}>
      <button type="button" className={styles.borderBtn} disabled={disabled} aria-label={strings.sheetBordersAdvanced} title={strings.sheetBordersAdvanced} aria-expanded={open} onClick={() => setOpen((current) => !current)}>
        <ChevronDown size={14} />
      </button>
      {open && (
        <div className={styles.advancedBorderPanel}>
          <BorderBtn kind="diag-down" label={strings.sheetBordersDiagonalDown} onClick={() => apply("diag-down")} disabled={disabled} />
          <BorderBtn kind="diag-up" label={strings.sheetBordersDiagonalUp} onClick={() => apply("diag-up")} disabled={disabled} />
          <BorderBtn kind="diag-down-center" label={strings.sheetBordersDiagonalDownCenter} onClick={() => apply("diag-down-center")} disabled={disabled} />
          <BorderBtn kind="diag-down-both" label={strings.sheetBordersDiagonalDownBoth} onClick={() => apply("diag-down-both")} disabled={disabled} />
          <BorderBtn kind="diag-up-center" label={strings.sheetBordersDiagonalUpCenter} onClick={() => apply("diag-up-center")} disabled={disabled} />
        </div>
      )}
    </div>
  );
}

function handleRibbonMenuToggle(event: React.SyntheticEvent<HTMLDetailsElement>) {
  const menu = event.currentTarget;
  if (!menu.open) return;
  document.querySelectorAll<HTMLDetailsElement>("details[data-ribbon-menu][open]").forEach((candidate) => {
    if (candidate !== menu) candidate.removeAttribute("open");
  });
  window.requestAnimationFrame(() => {
    const summary = menu.querySelector<HTMLElement>(":scope > summary");
    const panel = menu.querySelector<HTMLElement>("[data-ribbon-floating-panel]");
    if (summary === null || panel === null) return;
    const trigger = summary.getBoundingClientRect();
    const panelRect = panel.getBoundingClientRect();
    const gutter = 8;
    const left = Math.max(gutter, Math.min(trigger.left, window.innerWidth - panelRect.width - gutter));
    const below = trigger.bottom + 5;
    const top = below + panelRect.height <= window.innerHeight - gutter
      ? below
      : Math.max(gutter, trigger.top - panelRect.height - 5);
    panel.style.position = "fixed";
    panel.style.left = `${left}px`;
    panel.style.top = `${top}px`;
    panel.style.right = "auto";
  });
}

function BrandedSelect({ className, label, disabled, defaultValue, selectedValue, options, onSelect }: { className: string | undefined; label: string; disabled: boolean; defaultValue: string; selectedValue?: string | null; options: { value: string; label: string; preview?: string }[]; onSelect: (value: string) => void }) {
  const [value, setValue] = useState(defaultValue);
  useEffect(() => {
    if (selectedValue !== null && selectedValue !== undefined) setValue(selectedValue);
  }, [selectedValue]);
  const selected = options.find((option) => option.value === value)?.label ?? value;
  return (
    <details className={`${styles.brandedSelect} ${className ?? ""}`} data-ribbon-menu onToggle={handleRibbonMenuToggle}>
      <summary className={disabled ? styles.brandedSelectDisabled : undefined} aria-label={label} title={label}>
        <span>{selected}</span><ChevronDown size={14} />
      </summary>
      <div className={styles.brandedSelectPanel} data-ribbon-floating-panel>
        {options.map((option) => (
          <button key={option.value} type="button" className={option.value === value ? styles.brandedOptionActive : styles.brandedOption} onClick={(event) => {
            setValue(option.value);
            onSelect(option.value);
            event.currentTarget.closest("details")?.removeAttribute("open");
          }}>
            <span className={styles.brandedOptionContent}>
              <span className={styles.brandedOptionLabel}>{option.label}</span>
              {option.preview !== undefined && <span className={styles.brandedOptionPreview}>{option.preview}</span>}
            </span>
            <Check className={styles.brandedOptionCheck} size={15} aria-hidden="true" />
          </button>
        ))}
      </div>
    </details>
  );
}

function RibbonMenu({ label, icon, disabled, children, variant = "standard", formatKey }: { label: string; icon: React.ReactNode; disabled: boolean; children: React.ReactNode; variant?: "icon" | "row" | "standard" | "wide"; formatKey?: string }) {
  const variantClass = variant === "icon" ? styles.ribbonMenuIcon : variant === "row" ? styles.ribbonMenuRow : variant === "wide" ? styles.ribbonMenuWide : styles.ribbonMenuStandard;
  return (
    <details
      className={`${styles.ribbonMenu} ${variantClass}`}
      data-ribbon-menu
      onToggle={handleRibbonMenuToggle}
    >
      <summary className={disabled ? styles.ribbonMenuDisabled : undefined} data-format-key={formatKey} aria-label={label}>
        {icon}
        {variant === "icon" ? (
          <ChevronDown className={styles.menuIconChevron} size={12} />
        ) : (
          <span className={styles.menuCaption}>{label}<ChevronDown size={11} /></span>
        )}
      </summary>
      <div className={styles.ribbonMenuPanel} data-ribbon-floating-panel>{children}</div>
    </details>
  );
}

/** An Alo colour control shared with the rest of the application. */
function ColorBtn({
  label,
  onPick,
  disabled,
  children,
  formatKey,
  selectedColor = "#000000",
}: {
  label: string;
  onPick: (hex: string) => void;
  disabled: boolean;
  children: React.ReactNode;
  formatKey?: string;
  selectedColor?: string;
}) {
  const normalizedColor = /^#[0-9a-f]{6}$/i.test(selectedColor) ? selectedColor : "#000000";

  return (
    <ColorPicker
      label={label}
      value={normalizedColor}
      onChange={onPick}
      disabled={disabled}
      triggerIcon={children}
      {...(formatKey === undefined ? {} : { formatKey })}
      {...(styles.colorBtn === undefined ? {} : { className: styles.colorBtn })}
      triggerClassName="!size-8 !rounded-md !border-0 !bg-transparent"
    />
  );
}
