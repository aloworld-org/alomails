// Small shared pieces for the Drive UI: the icon for a node kind/type, a human
// file size, and the browser download helper.
import {
  File as FileIcon,
  FileSpreadsheet,
  FileText,
  Folder,
  Image as ImageIcon,
  Presentation,
  Table2,
  type LucideIcon,
} from "lucide-react";

import type { DriveNodeDto } from "../jmap/types";

/** Preserve a useful backend reason for Drive recovery messages. */
export function driveErrorReason(error: unknown): string | null {
  if (error instanceof Error && error.message.trim().length > 0) return error.message.trim();
  if (typeof error === "string" && error.trim().length > 0) return error.trim();
  return null;
}

/** The icon for a node, by kind and (for plain files) content type. */
export function nodeIcon(n: DriveNodeDto): LucideIcon {
  switch (n.kind) {
    case "folder":
      return Folder;
    case "doc":
      return FileText;
    case "sheet":
      return FileSpreadsheet;
    case "slides":
      return Presentation;
    case "base":
      return Table2;
    default:
      break;
  }
  const ct = n.contentType ?? "";
  if (ct.startsWith("image/")) return ImageIcon;
  if (ct === "application/pdf" || ct.startsWith("text/")) return FileText;
  if (ct.includes("spreadsheet") || ct.includes("excel")) return FileSpreadsheet;
  if (ct.includes("presentation") || ct.includes("powerpoint")) return Presentation;
  return FileIcon;
}

/** A human-readable byte size. */
export function fileSize(bytes: number): string {
  if (bytes <= 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/** Saves a blob to the user's machine under `name`. */
export function saveBlob(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
}
