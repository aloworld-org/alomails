import { useEffect, useRef, useState } from "react";
import { Bold, Italic } from "lucide-react";
import { strings } from "../../i18n";
import { RichTextCommand } from "./RichTextCommand";
import { sanitizeInlineRichText } from "./richText";

export function InlineRichTextEditor({
  value,
  placeholder,
  onChange,
  ...rest
}: {
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
  "aria-label": string;
}) {
  const editor = useRef<HTMLDivElement>(null);
  const lastEmitted = useRef("");
  const [showTools, setShowTools] = useState(false);

  useEffect(() => {
    if (editor.current !== null && value !== lastEmitted.current) {
      editor.current.innerHTML = sanitizeInlineRichText(value);
      lastEmitted.current = value;
    }
  }, [value]);

  const emit = () => {
    if (editor.current === null) return;
    const next = sanitizeInlineRichText(editor.current.innerHTML);
    lastEmitted.current = next;
    onChange(next);
  };
  const inspectSelection = () => {
    const selection = window.getSelection();
    const node = selection?.anchorNode;
    setShowTools(
      selection !== null &&
        !selection.isCollapsed &&
        node != null &&
        editor.current?.contains(node) === true,
    );
  };
  const command = (name: "bold" | "italic") => {
    editor.current?.focus();
    document.execCommand(name);
    emit();
    inspectSelection();
  };

  return (
    <div className="relative min-w-0">
      {showTools && (
        <div
          className="absolute bottom-[calc(100%+0.5rem)] left-3 z-20 flex items-center gap-1 rounded-xl border border-default bg-surface p-1.5 shadow-lg"
          role="toolbar"
          aria-label={strings.quoteStudioListItemFormatting}
          onMouseDown={(event) => event.preventDefault()}
        >
          <RichTextCommand label={strings.quoteStudioBold} onClick={() => command("bold")}>
            <Bold className="size-4" />
          </RichTextCommand>
          <RichTextCommand label={strings.quoteStudioItalic} onClick={() => command("italic")}>
            <Italic className="size-4" />
          </RichTextCommand>
        </div>
      )}
      <div
        ref={editor}
        contentEditable
        suppressContentEditableWarning
        role="textbox"
        data-placeholder={placeholder}
        className="min-h-11 w-full rounded-lg bg-transparent px-2 py-2.5 text-sm leading-6 text-primary transition-colors selection:bg-accent-soft selection:text-primary empty:before:pointer-events-none empty:before:text-tertiary empty:before:content-[attr(data-placeholder)] hover:bg-raised/50 focus:bg-accent-soft/30 focus:outline-none [&_strong]:font-semibold"
        onInput={emit}
        onMouseUp={inspectSelection}
        onKeyUp={inspectSelection}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.preventDefault();
        }}
        onBlur={() => {
          if (editor.current !== null) {
            const clean = sanitizeInlineRichText(editor.current.innerHTML);
            editor.current.innerHTML = clean;
            lastEmitted.current = clean;
            onChange(clean);
          }
          setShowTools(false);
        }}
        {...rest}
      />
    </div>
  );
}
