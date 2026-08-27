// A rich-text editor for compose: a contentEditable surface with a Google-Docs-
// style toolbar (text styles, bold/italic/underline/strikethrough, text + highlight
// colour, lists, alignment, quote, rule, link, image, equation, code, clear).
// Formatting uses the browser's built-in editing commands; it emits HTML on every
// edit, and the parent derives a plain-text alternative from it.
import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  Baseline,
  Bold,
  Eraser,
  Highlighter,
  Image as ImageIcon,
  Italic,
  Link2,
  List,
  ListOrdered,
  Minus,
  Quote,
  Strikethrough,
  Underline,
} from "lucide-react";

import { strings } from "../../i18n";
import { surface } from "../../product";
import { ColorPicker, IconButton, Select, Toolbar, ToolbarDivider, useDialogs } from "../../ds";
import styles from "./RichTextEditor.module.css";

/** Largest inline image edge (px); wider images are downscaled before embedding. */
const MAX_IMAGE_EDGE = 1400;

/** Font family choices (label → CSS stack), Gmail's set. */
const FONT_OPTIONS: { label: string; value: string }[] = [
  { label: "Sans Serif", value: "Arial, Helvetica, sans-serif" },
  { label: "Serif", value: "Georgia, 'Times New Roman', serif" },
  { label: "Fixed Width", value: "'Courier New', monospace" },
  { label: "Wide", value: "'Arial Black', sans-serif" },
  { label: "Narrow", value: "'Arial Narrow', sans-serif" },
  { label: "Comic Sans", value: "'Comic Sans MS', cursive" },
  { label: "Garamond", value: "Garamond, serif" },
  { label: "Georgia", value: "Georgia, serif" },
  { label: "Tahoma", value: "Tahoma, sans-serif" },
  { label: "Trebuchet", value: "'Trebuchet MS', sans-serif" },
  { label: "Verdana", value: "Verdana, sans-serif" },
];

const DEFAULT_FONT = FONT_OPTIONS[0]!.value;
const SIZE_VALUES = new Set(["2", "3", "5", "7"]);

const normalizeFont = (s: string): string => s.toLowerCase().replace(/["']/g, "").replace(/\s+/g, "");

/** Best-effort map from a browser-reported font-family to one of our options. */
function matchFont(reported: string): string {
  if (reported === "") return DEFAULT_FONT;
  const norm = normalizeFont(reported);
  const exact = FONT_OPTIONS.find((o) => normalizeFont(o.value) === norm);
  if (exact !== undefined) return exact.value;
  const byFirst = FONT_OPTIONS.find((o) => {
    const first = normalizeFont(o.value.split(",")[0] ?? "");
    return first !== "" && norm.startsWith(first);
  });
  return byFirst?.value ?? DEFAULT_FONT;
}

interface RichTextEditorProps {
  /** Initial HTML (uncontrolled thereafter — set once on mount). */
  initialHtml: string;
  /** Called with the editor's current HTML on every edit. */
  onChange: (html: string) => void;
  placeholder: string;
  autoFocus?: boolean;
}

/** Load a data URL into an Image element. */
function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = src;
  });
}

/** Read a file, downscaling large images so the embedded data URI stays sane. */
async function imageDataUrl(file: File): Promise<string> {
  const raw = await new Promise<string>((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = reject;
    r.readAsDataURL(file);
  });
  try {
    const img = await loadImage(raw);
    const longest = Math.max(img.width, img.height);
    if (longest <= MAX_IMAGE_EDGE) return raw;
    const scale = MAX_IMAGE_EDGE / longest;
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(img.width * scale);
    canvas.height = Math.round(img.height * scale);
    const ctx = canvas.getContext("2d");
    if (ctx === null) return raw;
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    const type = file.type === "image/png" ? "image/png" : "image/jpeg";
    return canvas.toDataURL(type, 0.85);
  } catch {
    return raw;
  }
}

export function RichTextEditor({ initialHtml, onChange, placeholder, autoFocus }: RichTextEditorProps) {
  const { prompt } = useDialogs();
  const ref = useRef<HTMLDivElement>(null);
  const savedRange = useRef<Range | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  // The id of the open compose-insert (from the product surface), or null.
  const [insert, setInsert] = useState<string | null>(null);
  const [font, setFont] = useState(DEFAULT_FONT);
  // Compose-editor inserts (e.g. equation/code) are contributed by the product
  // surface (ADR 0019): the mail product has none, so the mail build carries no
  // KaTeX/Prism; the workspace supplies them. Read at render, not module scope
  // (the surface ↔ mail import cycle leaves it unset at module init).
  const composeInserts = surface.composeInserts;
  const [size, setSize] = useState("3");
  const [textColor, setTextColor] = useState("#102A43");
  const [highlightColor, setHighlightColor] = useState("#FFF2A8");

  useEffect(() => {
    const el = ref.current;
    if (el === null) return;
    el.innerHTML = initialHtml;
    if (autoFocus === true) el.focus();
  }, [initialHtml, autoFocus]);

  // Reflect the caret's font + size in the dropdowns (Gmail syncs these as you move).
  useEffect(() => {
    function sync() {
      const el = ref.current;
      if (el === null) return;
      const sel = window.getSelection();
      if (sel === null || sel.rangeCount === 0) return;
      if (!el.contains(sel.getRangeAt(0).commonAncestorContainer)) return;
      try {
        setFont(matchFont(String(document.queryCommandValue("fontName"))));
        const fs = String(document.queryCommandValue("fontSize"));
        setSize(SIZE_VALUES.has(fs) ? fs : "3");
      } catch {
        // queryCommandValue can throw in odd states — ignore
      }
    }
    document.addEventListener("selectionchange", sync);
    return () => document.removeEventListener("selectionchange", sync);
  }, []);

  function emit() {
    onChange(ref.current?.innerHTML ?? "");
  }

  /** Remember the caret so a control that steals focus (colour picker, file
   * dialog, insert modal) can restore where the user was. */
  function saveRange() {
    const sel = window.getSelection();
    if (
      sel !== null &&
      sel.rangeCount > 0 &&
      ref.current?.contains(sel.getRangeAt(0).commonAncestorContainer) === true
    ) {
      savedRange.current = sel.getRangeAt(0).cloneRange();
    }
  }

  function restoreRange() {
    const el = ref.current;
    if (el === null) return;
    el.focus();
    const sel = window.getSelection();
    if (sel === null || savedRange.current === null) return;
    sel.removeAllRanges();
    sel.addRange(savedRange.current);
  }

  /** Run an editing command with the selection intact (toolbar buttons keep it
   * via preventDefault on mousedown). */
  function exec(command: string, value?: string) {
    ref.current?.focus();
    document.execCommand(command, false, value);
    emit();
  }

  /** Run a command that needs the pre-blur selection restored first. */
  function execRestored(command: string, value: string) {
    restoreRange();
    document.execCommand(command, false, value);
    emit();
  }

  async function addLink() {
    saveRange();
    const url = await prompt({ message: strings.linkPrompt });
    if (url === null || url.trim().length === 0) return;
    execRestored("createLink", url.trim());
  }

  function openInsert(id: string) {
    saveRange();
    setInsert(id);
  }

  /** Insert HTML at the saved caret, parsed via <template> so MathML/atoms survive. */
  function insertHtml(html: string) {
    setInsert(null);
    const el = ref.current;
    if (el === null) return;
    el.focus();
    const sel = window.getSelection();
    if (sel === null) return;
    let range: Range;
    if (savedRange.current !== null) {
      range = savedRange.current;
    } else {
      range = document.createRange();
      range.selectNodeContents(el);
      range.collapse(false);
    }
    range.deleteContents();
    const tpl = document.createElement("template");
    tpl.innerHTML = `${html}&nbsp;`;
    const lastNode = tpl.content.lastChild;
    range.insertNode(tpl.content);
    if (lastNode !== null) {
      const after = document.createRange();
      after.setStartAfter(lastNode);
      after.collapse(true);
      sel.removeAllRanges();
      sel.addRange(after);
    }
    emit();
  }

  function insertImageDataUrl(dataUrl: string) {
    insertHtml(`<img src="${dataUrl}" alt="" style="max-width:100%;height:auto" />`);
  }

  async function onPickImage(file: File) {
    if (!file.type.startsWith("image/")) return;
    insertImageDataUrl(await imageDataUrl(file));
  }

  // Paste an image straight into the body (Outlook-style): a screenshot or a
  // copied image lands where the caret is. Non-image pastes fall through to the
  // browser's normal text/HTML paste.
  async function onPaste(e: React.ClipboardEvent<HTMLDivElement>) {
    const items = e.clipboardData?.items;
    if (items === undefined) return;
    const images = Array.from(items).filter(
      (it) => it.kind === "file" && it.type.startsWith("image/"),
    );
    if (images.length === 0) return;
    e.preventDefault();
    for (const it of images) {
      const file = it.getAsFile();
      if (file === null) continue;
      const dataUrl = await imageDataUrl(file);
      saveRange();
      insertImageDataUrl(dataUrl);
    }
  }

  // Drag-and-drop image files onto the body, inserting at the drop point.
  async function onDrop(e: React.DragEvent<HTMLDivElement>) {
    const files = Array.from(e.dataTransfer?.files ?? []).filter((f) =>
      f.type.startsWith("image/"),
    );
    if (files.length === 0) return;
    e.preventDefault();
    // Place the caret where the image was dropped.
    const doc = document as Document & {
      caretRangeFromPoint?: (x: number, y: number) => Range | null;
    };
    const range = doc.caretRangeFromPoint?.(e.clientX, e.clientY) ?? null;
    if (range !== null) savedRange.current = range;
    for (const file of files) {
      const dataUrl = await imageDataUrl(file);
      insertImageDataUrl(dataUrl);
      saveRange();
    }
  }

  /** A toolbar button (keeps the editor selection via mousedown preventDefault:
   *  without it the contentEditable blurs and the caret — which is what every
   *  one of these commands acts on — is gone before the click lands). */
  function tool(key: string, label: string, icon: ReactNode, onClick: () => void) {
    return (
      <IconButton
        key={key}
        label={label}
        icon={icon}
        onMouseDown={(e) => e.preventDefault()}
        onClick={onClick}
      />
    );
  }

  const divider = (k: string) => <ToolbarDivider key={k} />;

  return (
    <div className={styles.wrap}>
      <Toolbar
        label={strings.formatting}
        surface="bar"
        density="compact"
        // `tab`, not `roving`: the row holds two selects and two colour
        // pickers, and arrow keys inside those belong to the control, not to
        // the toolbar. It was announced as `role="toolbar"` with no arrow keys
        // at all, which is the promise `ds/Toolbar` exists to stop making.
        keyboard="tab"
      >
        <Select
          variant="ghost"
          className={styles.font}
          aria-label={strings.fontFamily}
          value={font}
          onMouseDown={saveRange}
          onChange={(e) => {
            setFont(e.target.value);
            execRestored("fontName", e.target.value);
          }}
        >
          {FONT_OPTIONS.map((o) => (
            <option key={o.label} value={o.value}>
              {o.label}
            </option>
          ))}
        </Select>
        <Select
          variant="ghost"
          className={styles.fontSize}
          aria-label={strings.fontSize}
          value={size}
          onMouseDown={saveRange}
          onChange={(e) => {
            setSize(e.target.value);
            execRestored("fontSize", e.target.value);
          }}
        >
          <option value="2">{strings.sizeSmall}</option>
          <option value="3">{strings.sizeNormal}</option>
          <option value="5">{strings.sizeLarge}</option>
          <option value="7">{strings.sizeHuge}</option>
        </Select>
        {divider("d0")}

        {tool("bold", strings.bold, <Bold size={16} />, () => exec("bold"))}
        {tool("italic", strings.italic, <Italic size={16} />, () => exec("italic"))}
        {tool("underline", strings.underline, <Underline size={16} />, () => exec("underline"))}
        {tool("strike", strings.strikethrough, <Strikethrough size={16} />, () =>
          exec("strikeThrough"),
        )}

        <ColorPicker
          label={strings.textColor}
          value={textColor}
          triggerIcon={<Baseline size={16} />}
          onPointerDown={saveRange}
          onChange={(next) => {
            setTextColor(next);
            execRestored("foreColor", next);
          }}
        />
        <ColorPicker
          label={strings.highlight}
          value={highlightColor}
          triggerIcon={<Highlighter size={16} />}
          onPointerDown={saveRange}
          onChange={(next) => {
            setHighlightColor(next);
            execRestored("hiliteColor", next);
          }}
        />
        {divider("d1")}

        {tool("ul", strings.bulletList, <List size={16} />, () => exec("insertUnorderedList"))}
        {tool("ol", strings.numberedList, <ListOrdered size={16} />, () =>
          exec("insertOrderedList"),
        )}
        {tool("alignL", strings.alignLeft, <AlignLeft size={16} />, () => exec("justifyLeft"))}
        {tool("alignC", strings.alignCenter, <AlignCenter size={16} />, () => exec("justifyCenter"))}
        {tool("alignR", strings.alignRight, <AlignRight size={16} />, () => exec("justifyRight"))}
        {divider("d2")}

        {tool("quote", strings.styleQuote, <Quote size={16} />, () =>
          exec("formatBlock", "blockquote"),
        )}
        {tool("hr", strings.horizontalRule, <Minus size={16} />, () => exec("insertHorizontalRule"))}
        {tool("link", strings.link, <Link2 size={16} />, addLink)}
        {tool("image", strings.insertImage, <ImageIcon size={16} />, () => {
          saveRange();
          fileInput.current?.click();
        })}
        {divider("d3")}

        {composeInserts.map((ci) =>
          tool(ci.id, ci.label, <ci.Icon size={16} />, () => openInsert(ci.id)),
        )}
        {tool("clear", strings.clearFormatting, <Eraser size={16} />, () => exec("removeFormat"))}
      </Toolbar>

      <input
        ref={fileInput}
        type="file"
        accept="image/*"
        className={styles.fileInput}
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f !== undefined) void onPickImage(f);
          e.target.value = "";
        }}
      />

      <div
        ref={ref}
        className={styles.editor}
        contentEditable
        role="textbox"
        aria-multiline="true"
        aria-label={placeholder}
        data-placeholder={placeholder}
        onInput={emit}
        onBlur={saveRange}
        onPaste={(e) => void onPaste(e)}
        onDrop={(e) => void onDrop(e)}
        suppressContentEditableWarning
      />
      {(() => {
        if (insert === null) return null;
        const ci = composeInserts.find((c) => c.id === insert);
        if (ci === undefined) return null;
        const Modal = ci.Modal;
        return <Modal onInsert={insertHtml} onClose={() => setInsert(null)} />;
      })()}
    </div>
  );
}
