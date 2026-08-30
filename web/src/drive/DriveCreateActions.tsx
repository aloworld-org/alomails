import {
  ChevronDown,
  FileSpreadsheet,
  FileText,
  FileType,
  FolderPlus,
  Presentation,
  Upload,
} from "lucide-react";
import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";

export interface DriveCreateActionsLabels {
  createDocument: string;
  aloDocument: string;
  sheet: string;
  word: string;
  slides: string;
  folder: string;
  upload: string;
}

interface DriveCreateActionsProps {
  labels: DriveCreateActionsLabels;
  onCreateDocument: () => void;
  onCreateSheet: () => void;
  onCreateWord: () => void;
  onCreateSlides: () => void;
  onCreateFolder: () => void;
  onUpload: () => void;
  uploadDisabled?: boolean;
  align?: "start" | "end";
}

export function DriveCreateActions({
  labels,
  onCreateDocument,
  onCreateSheet,
  onCreateWord,
  onCreateSlides,
  onCreateFolder,
  onUpload,
  uploadDisabled = false,
  align = "end",
}: DriveCreateActionsProps) {
  const [open, setOpen] = useState(false);
  const [menuPosition, setMenuPosition] = useState<{
    left: number;
    top: number;
  } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const dismiss = useCallback(() => setOpen(false), []);

  const positionMenu = useCallback(() => {
    const trigger = rootRef.current;
    const menu = menuRef.current;
    if (trigger === null || menu === null) return;

    const viewportPadding = 12;
    const gap = 8;
    const triggerRect = trigger.getBoundingClientRect();
    const menuRect = menu.getBoundingClientRect();
    const preferredLeft =
      align === "start" ? triggerRect.left : triggerRect.right - menuRect.width;
    const left = Math.min(
      Math.max(preferredLeft, viewportPadding),
      Math.max(
        viewportPadding,
        window.innerWidth - menuRect.width - viewportPadding,
      ),
    );
    const fitsBelow =
      triggerRect.bottom + gap + menuRect.height <=
      window.innerHeight - viewportPadding;
    const top = fitsBelow
      ? triggerRect.bottom + gap
      : Math.max(viewportPadding, triggerRect.top - menuRect.height - gap);

    setMenuPosition({ left, top });
  }, [align]);

  useLayoutEffect(() => {
    if (!open) {
      setMenuPosition(null);
      return;
    }
    positionMenu();
  }, [open, positionMenu]);

  useEffect(() => {
    if (!open) return undefined;

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (
        !rootRef.current?.contains(target) &&
        !menuRef.current?.contains(target)
      ) {
        dismiss();
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") dismiss();
    };

    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", positionMenu);
    window.addEventListener("scroll", positionMenu, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("resize", positionMenu);
      window.removeEventListener("scroll", positionMenu, true);
    };
  }, [dismiss, open, positionMenu]);

  const run = (action: () => void) => {
    setOpen(false);
    action();
  };

  const options = [
    {
      key: "alo-document",
      icon: FileText,
      label: labels.aloDocument,
      action: onCreateDocument,
      separated: false,
      disabled: false,
    },
    {
      key: "sheet",
      icon: FileSpreadsheet,
      label: labels.sheet,
      action: onCreateSheet,
      separated: false,
      disabled: false,
    },
    {
      key: "word",
      icon: FileType,
      label: labels.word,
      action: onCreateWord,
      separated: false,
      disabled: false,
    },
    {
      key: "slides",
      icon: Presentation,
      label: labels.slides,
      action: onCreateSlides,
      separated: false,
      disabled: false,
    },
    {
      key: "folder",
      icon: FolderPlus,
      label: labels.folder,
      action: onCreateFolder,
      separated: true,
      disabled: false,
    },
    {
      key: "upload",
      icon: Upload,
      label: labels.upload,
      action: onUpload,
      separated: false,
      disabled: uploadDisabled,
    },
  ];

  return (
    <div ref={rootRef} className="relative inline-flex items-center">
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
        className="inline-flex h-10 items-center justify-center gap-2 rounded-xl bg-[#E76F51] px-4 text-sm font-medium text-white shadow-[0_1px_2px_rgba(16,42,67,0.06)] transition-all duration-150 ease-out hover:bg-[#D96247] active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50 focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-[#E76F51]/15"
      >
        <FileText size={16} aria-hidden="true" />
        {labels.createDocument}
        <ChevronDown
          size={16}
          aria-hidden="true"
          className={`transition-transform duration-150 ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open &&
        createPortal(
          <div
            ref={menuRef}
            role="menu"
            aria-label={labels.createDocument}
            onPointerDown={(event) => event.stopPropagation()}
            style={
              menuPosition === null
                ? { left: 0, top: 0, visibility: "hidden" }
                : menuPosition
            }
            className="fixed z-[1000] w-64 isolate overflow-y-auto rounded-2xl border border-[#E8DED2] bg-[#FFFEFC] p-2 shadow-[0_12px_30px_rgba(16,42,67,0.12)] [max-height:calc(100vh-1.5rem)]"
          >
            {options.map((option) => {
              const Icon = option.icon;
              return (
                <Fragment key={option.key}>
                  {option.separated && (
                    <div className="my-1 h-px bg-[#E8DED2]" />
                  )}
                  <button
                    type="button"
                    role="menuitem"
                    disabled={option.disabled}
                    onClick={() => run(option.action)}
                    className="flex h-11 w-full items-center gap-3 rounded-xl px-3 text-left text-sm font-medium text-[#102A43] transition-colors duration-150 hover:bg-[#E76F51]/10 hover:text-[#D96247] disabled:pointer-events-none disabled:opacity-50 focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-[#E76F51]/15"
                  >
                    <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-[#E76F51]/10 text-[#E76F51]">
                      <Icon size={16} aria-hidden="true" />
                    </span>
                    {option.label}
                  </button>
                </Fragment>
              );
            })}
          </div>,
          document.body,
        )}
    </div>
  );
}
