import {
  ChevronDown,
  FileSpreadsheet,
  FileText,
  FileType,
  FolderPlus,
  Presentation,
  Upload,
} from "lucide-react";
import { Fragment, useCallback, useRef, useState } from "react";

import { useDismiss } from "../ds/useDismiss";

export interface DriveCreateActionsLabels {
  createDocument: string;
  more: string;
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
  const rootRef = useRef<HTMLDivElement>(null);
  const dismiss = useCallback(() => setOpen(false), []);
  useDismiss(open, rootRef, dismiss);

  const run = (action: () => void) => {
    setOpen(false);
    action();
  };

  const options = [
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
        onClick={onCreateDocument}
        className="inline-flex h-10 items-center justify-center gap-2 rounded-l-xl bg-[#E76F51] px-4 text-sm font-medium text-white shadow-[0_1px_2px_rgba(16,42,67,0.06)] transition-all duration-150 ease-out hover:bg-[#D96247] active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50 focus-visible:z-10 focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-[#E76F51]/15"
      >
        <FileText size={16} aria-hidden="true" />
        {labels.createDocument}
      </button>
      <button
        type="button"
        aria-label={labels.more}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
        className="inline-flex h-10 items-center justify-center rounded-r-xl border-l border-white/25 bg-[#E76F51] px-3 text-white shadow-[0_1px_2px_rgba(16,42,67,0.06)] transition-all duration-150 ease-out hover:bg-[#D96247] active:scale-[0.98] focus-visible:z-10 focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-[#E76F51]/15"
      >
        <ChevronDown
          size={16}
          aria-hidden="true"
          className={open ? "rotate-180" : ""}
        />
      </button>
      {open && (
        <div
          role="menu"
          aria-label={labels.more}
          className={`absolute top-[calc(100%+0.5rem)] z-50 min-w-64 rounded-2xl border border-[#E8DED2] bg-white p-2 shadow-[0_12px_30px_rgba(16,42,67,0.12)] ${align === "start" ? "left-0" : "right-0"}`}
        >
          {options.map((option) => {
            const Icon = option.icon;
            return (
              <Fragment key={option.key}>
                {option.separated && <div className="my-1 h-px bg-[#E8DED2]" />}
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
        </div>
      )}
    </div>
  );
}
