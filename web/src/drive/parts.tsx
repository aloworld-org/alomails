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

import type { RecordOrigin } from "../agents";
import type { DriveNodeDto } from "../jmap/types";

/** Where a Drive node came from, in the provenance shape the record agent
 *  panel renders (A8.4).
 *
 *  A node carries the source it was saved from — the email an attachment was
 *  filed out of, the conversation a shared file was kept from — which is the
 *  origin whenever it is there. `createdBy` is deliberately not a fallback:
 *  it holds the account id, an opaque string a person cannot read, and a
 *  panel that printed it would be citing a source nobody can follow. When the
 *  read side adopts the stored `record_origins` join, that field arrives with
 *  a name and this function passes it through instead. */
export function driveNodeOrigin(
  node: Pick<DriveNodeDto, "sourceKind" | "sourceId">,
): RecordOrigin | null {
  if (node.sourceId === null) return null;
  switch (node.sourceKind) {
    case "chat":
      return { kind: "thread", id: node.sourceId, label: null };
    case "email":
      return { kind: "message", id: node.sourceId, label: null };
    case "event":
      return { kind: "event", id: node.sourceId, label: null };
    default:
      return null;
  }
}

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
