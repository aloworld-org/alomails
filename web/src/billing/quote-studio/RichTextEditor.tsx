import { useEffect, useRef, useState } from "react";
import { Bold, Heading1, Heading2, Heading3, Italic, List, ListOrdered, Pilcrow, Type } from "lucide-react";
import { cx } from "../../ds";
import { strings } from "../../i18n";
import { RichTextCommand } from "./RichTextCommand";
import { sanitizeRichText } from "./richText";

export function RichTextEditor({
  value,
  label = strings.quoteStudioSupportingText,
  placeholder,
  onChange,
}: {
  value: string;
  label?: string;
  placeholder: string;
  onChange: (value: string) => void;
}) {
  const editor = useRef<HTMLDivElement>(null);
  const lastEmitted = useRef("");
  const [showTools, setShowTools] = useState(false);

  useEffect(() => {
    if (editor.current !== null && value !== lastEmitted.current) {
      editor.current.innerHTML = sanitizeRichText(value);
      lastEmitted.current = value;
    }
  }, [value]);

  const emit = () => {
    if (editor.current === null) return;
    const next = editor.current.innerHTML;
    lastEmitted.current = next;
    onChange(next);
  };
  const inspectSelection = () => {
    const selection = window.getSelection();
    const node = selection?.anchorNode;
    setShowTools(selection !== null && !selection.isCollapsed && node != null && editor.current?.contains(node) === true);
  };
  const command = (name: string, argument?: string) => {
    editor.current?.focus();
    document.execCommand(name, false, argument);
    emit();
    inspectSelection();
  };

  const commands = [
    [strings.quoteStudioBold, <Bold className="size-4" />, () => command("bold")],
    [strings.quoteStudioItalic, <Italic className="size-4" />, () => command("italic")],
    [strings.quoteStudioHeading1, <Heading1 className="size-4" />, () => command("formatBlock", "h1")],
    [strings.quoteStudioHeading2, <Heading2 className="size-4" />, () => command("formatBlock", "h2")],
    [strings.quoteStudioHeading3, <Heading3 className="size-4" />, () => command("formatBlock", "h3")],
    [strings.quoteStudioParagraph, <Pilcrow className="size-4" />, () => command("formatBlock", "p")],
    [strings.quoteStudioBulletList, <List className="size-4" />, () => command("insertUnorderedList")],
    [strings.quoteStudioNumberedList, <ListOrdered className="size-4" />, () => command("insertOrderedList")],
  ] as const;

  return (
    <div>
      <div className="mb-2 flex items-center justify-between gap-3">
        <p className="text-sm font-semibold text-primary">{label}</p>
        <button
          type="button"
          className={cx("inline-flex min-h-9 items-center gap-2 rounded-lg border px-3 text-xs font-semibold transition-colors", showTools ? "border-accent bg-accent-soft text-accent" : "border-default bg-surface text-secondary hover:border-accent hover:bg-accent-soft hover:text-accent")}
          aria-expanded={showTools}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => setShowTools((current) => !current)}
        >
          <Type className="size-4" aria-hidden="true" /> {strings.quoteStudioTextTools}
        </button>
      </div>
      <div className="relative">
        {showTools && (
          <div className="absolute -top-12 left-1/2 z-10 flex -translate-x-1/2 items-center gap-1 rounded-xl border border-default bg-surface p-1.5 shadow-lg" role="toolbar" aria-label={strings.quoteStudioTextFormatting} onMouseDown={(event) => event.preventDefault()}>
            {commands.map(([commandLabel, icon, run]) => <RichTextCommand key={commandLabel} label={commandLabel} onClick={run}>{icon}</RichTextCommand>)}
          </div>
        )}
        <div
          ref={editor}
          contentEditable
          suppressContentEditableWarning
          role="textbox"
          aria-multiline="true"
          aria-label={label}
          data-placeholder={placeholder}
          className="min-h-32 w-full overflow-y-auto rounded-lg bg-transparent px-2 py-3 text-sm font-normal leading-relaxed text-primary transition-colors selection:bg-accent-soft selection:text-primary empty:before:pointer-events-none empty:before:text-tertiary empty:before:content-[attr(data-placeholder)] hover:bg-raised/50 focus:bg-accent-soft/30 focus:outline-none [&_h1]:my-2 [&_h1]:text-2xl [&_h1]:font-semibold [&_h2]:my-2 [&_h2]:text-xl [&_h2]:font-semibold [&_h3]:my-2 [&_h3]:text-lg [&_h3]:font-semibold [&_ol]:list-decimal [&_ol]:pl-6 [&_strong]:font-semibold [&_ul]:list-disc [&_ul]:pl-6"
          onInput={emit}
          onMouseUp={inspectSelection}
          onKeyUp={inspectSelection}
          onBlur={() => {
            if (editor.current !== null) {
              const clean = sanitizeRichText(editor.current.innerHTML);
              editor.current.innerHTML = clean;
              lastEmitted.current = clean;
              onChange(clean);
            }
            setShowTools(false);
          }}
        />
      </div>
    </div>
  );
}
