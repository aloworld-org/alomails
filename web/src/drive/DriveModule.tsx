// alo Drive — the file manager. Left: locations (My Files + the Spaces you
// belong to) and Trash. Right: the current folder's contents with a breadcrumb
// and per-item actions. Every file lives in one location; its access is that
// location's access (ADR 0027), so there is no per-file sharing here — sharing
// is membership of the Space it lives in, always visible via "Members".
import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import {
  ChevronRight,
  Copy,
  Download,
  FileText,
  FileType,
  FolderPlus,
  Presentation,
  Sheet,
  HardDrive,
  History,
  MoveRight,
  Pencil,
  Plus,
  RotateCcw,
  Trash2,
  Upload,
  Users,
  X,
} from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type DriveNodeDto, type SpaceDto } from "../jmap";
import { Menu, Spinner, useDialogs, type MenuItem } from "../ds";
import { DestinationDialog, MembersDialog, VersionsDialog } from "./dialogs";
import { blankOfficeFile, type OfficeExt } from "./blankTemplates";
// BlockNote is heavy and only needed when a doc opens — code-split it out.
const DocEditor = lazy(() => import("./DocEditor").then((m) => ({ default: m.DocEditor })));
// Univer is heavy; the native Sheet editor only loads when a sheet is opened.
const SheetEditor = lazy(() => import("./SheetEditor").then((m) => ({ default: m.SheetEditor })));
const OfficeEditor = lazy(() => import("./OfficeEditor").then((m) => ({ default: m.OfficeEditor })));

/** Real Office files open in Collabora; kept here so it doesn't pull the editor
 *  into the main bundle. */
const OFFICE_EXT = /\.(docx?|xlsx?|pptx?|odt|ods|odp|rtf|csv)$/i;
// Spreadsheets we import natively into alo Sheet instead of opening in Collabora
// (ADR 0033, stage 1). `.xlsx`/`.xlsm` are OOXML; `.xls` (old binary) and `.ods`
// are not covered yet and still fall through to the Office path.
const SPREADSHEET_IMPORT = /\.xls[mx]$/i;
import { fileSize, nodeIcon, saveBlob } from "./parts";
import { xlsxToUniverSnapshot } from "./importOffice";
import styles from "./DriveModule.module.css";

type Crumb = { id: string; name: string };

export function DriveModule() {
  const client = useJmapClient();
  const { prompt, confirm } = useDialogs();

  const [spaces, setSpaces] = useState<SpaceDto[]>([]);
  const [location, setLocation] = useState<string | null>(null); // null = My Files
  const [trashView, setTrashView] = useState(false);
  const [path, setPath] = useState<Crumb[]>([]);
  const [nodes, setNodes] = useState<DriveNodeDto[] | null>(null);
  const [uploading, setUploading] = useState(false);
  const [dragOver, setDragOver] = useState(false);

  const [moveNode, setMoveNode] = useState<{ id: string; mode: "move" | "copy" } | null>(null);
  const [versionsNode, setVersionsNode] = useState<string | null>(null);
  const [openDoc, setOpenDoc] = useState<{ id: string; name: string } | null>(null);
  const [openSheet, setOpenSheet] = useState<{ id: string; name: string } | null>(null);
  const [openOffice, setOpenOffice] = useState<{ id: string; name: string } | null>(null);
  // Best-effort import of a real Office file into a native editor (ADR 0033).
  const [importing, setImporting] = useState<string | null>(null);
  const [importFailed, setImportFailed] = useState<string | null>(null);
  const [showMembers, setShowMembers] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  const parent = path.length > 0 ? (path[path.length - 1]?.id ?? null) : null;
  const currentSpace = useMemo(() => spaces.find((s) => s.id === location) ?? null, [spaces, location]);
  const canWrite = location === null || (currentSpace !== null && currentSpace.myRole !== "viewer");

  const loadSpaces = useCallback(() => {
    void client.spaces().then(setSpaces).catch(() => setSpaces([]));
  }, [client]);

  const load = useCallback(async () => {
    try {
      const list = trashView
        ? await client.driveTrash(location)
        : await client.driveList(location, parent);
      setNodes(list);
    } catch {
      setNodes([]);
    }
  }, [client, location, parent, trashView]);

  useEffect(loadSpaces, [loadSpaces]);
  useEffect(() => {
    setNodes(null);
    void load();
  }, [load]);

  // Open a node arrived at from workspace search (?open=<id>&space=<id>).
  const [searchParams, setSearchParams] = useSearchParams();
  useEffect(() => {
    const id = searchParams.get("open");
    if (id === null) return;
    const sp = searchParams.get("space");
    const next = new URLSearchParams(searchParams);
    next.delete("open");
    next.delete("space");
    setSearchParams(next, { replace: true });
    setLocation(sp);
    setTrashView(false);
    setPath([]);
    void client.driveNode(id).then((node) => {
      if (node === null) return;
      if (node.kind === "folder") setPath([{ id: node.id, name: node.name }]);
      else if (node.kind === "doc") setOpenDoc({ id, name: node.name });
      else if (node.kind === "sheet") setOpenSheet({ id, name: node.name });
      else if (node.kind === "file" && SPREADSHEET_IMPORT.test(node.name))
        void importSpreadsheet(id, node.name);
      else if (node.kind === "file" && OFFICE_EXT.test(node.name)) setOpenOffice({ id, name: node.name });
    });
  }, [searchParams, setSearchParams, client]);

  function selectLocation(space: string | null) {
    setLocation(space);
    setTrashView(false);
    setPath([]);
  }

  function openNode(n: DriveNodeDto) {
    if (n.kind === "folder") setPath((p) => [...p, { id: n.id, name: n.name }]);
    else if (n.kind === "doc") setOpenDoc({ id: n.id, name: n.name });
    else if (n.kind === "sheet") setOpenSheet({ id: n.id, name: n.name });
    else if (n.kind === "file" && SPREADSHEET_IMPORT.test(n.name)) void importSpreadsheet(n.id, n.name);
    else if (n.kind === "file" && OFFICE_EXT.test(n.name)) setOpenOffice({ id: n.id, name: n.name });
    else void download(n);
  }

  async function newDoc() {
    const name = (await prompt({ message: strings.driveNewDocPrompt }))?.trim();
    if (!name) return;
    try {
      const id = await client.driveCreateDoc(location, parent, name);
      await load();
      setOpenDoc({ id, name });
    } catch {
      /* ignore */
    }
  }

  async function newSheet() {
    const name = (await prompt({ message: strings.driveNewSheetPrompt }))?.trim();
    if (!name) return;
    try {
      const id = await client.driveCreateSheet(location, parent, name);
      await load();
      setOpenSheet({ id, name });
    } catch {
      /* ignore */
    }
  }

  /** Import a real `.xlsx` into a new alo Sheet (ADR 0033, stage 1): convert its
   *  cells to a Univer snapshot, create a native sheet node, and open it. One-way
   *  — the original file is left untouched in Drive. */
  async function importSpreadsheet(fileId: string, fileName: string) {
    const base = fileName.replace(/\.[^.]+$/, "") || fileName;
    setImportFailed(null);
    setImporting(base);
    try {
      const bytes = new Uint8Array(await (await client.driveDownload(fileId)).arrayBuffer());
      const snapshot = xlsxToUniverSnapshot(bytes, base);
      const id = await client.driveCreateSheet(location, parent, base);
      await client.driveSaveSheet(id, snapshot);
      await load();
      setOpenSheet({ id, name: base });
    } catch {
      setImportFailed(fileName);
    } finally {
      setImporting(null);
    }
  }

  /** Create a blank Office document (Word/Excel/PowerPoint) from a template and
   *  open it in the Collabora editor — the two-file-types rule (ADR 0030). */
  async function newOffice(ext: OfficeExt) {
    const kind =
      ext === "docx" ? strings.driveKindWord : ext === "xlsx" ? strings.driveKindExcel : strings.driveKindSlides;
    const name = (await prompt({ message: strings.driveNameNew(kind) }))?.trim();
    if (!name) return;
    try {
      const file = blankOfficeFile(ext, name);
      const id = await client.driveUpload(location, parent, file);
      await load();
      setOpenOffice({ id, name: file.name });
    } catch {
      /* ignore */
    }
  }

  async function download(n: DriveNodeDto) {
    if (n.blobId === null) return;
    try {
      saveBlob(await client.driveDownload(n.id), n.name);
    } catch {
      /* ignore */
    }
  }

  async function uploadFiles(files: FileList | File[]) {
    setUploading(true);
    try {
      for (const f of Array.from(files)) {
        await client.driveUpload(location, parent, f);
      }
      await load();
    } catch {
      /* leave as-is */
    } finally {
      setUploading(false);
    }
  }

  async function newFolder() {
    const name = (await prompt({ message: strings.driveNewFolderPrompt }))?.trim();
    if (!name) return;
    try {
      await client.driveCreateFolder(location, parent, name);
      await load();
    } catch {
      /* ignore */
    }
  }

  async function newSpace() {
    const name = (await prompt({ message: strings.driveNewSpacePrompt }))?.trim();
    if (!name) return;
    try {
      const id = await client.createSpace(name);
      loadSpaces();
      selectLocation(id);
    } catch {
      /* ignore */
    }
  }

  async function rename(n: DriveNodeDto) {
    const name = (await prompt({ message: strings.driveRenamePrompt, defaultValue: n.name }))?.trim();
    if (!name || name === n.name) return;
    try {
      await client.driveRename(n.id, name);
      await load();
    } catch {
      /* ignore */
    }
  }

  async function trash(n: DriveNodeDto) {
    if (!(await confirm({ message: strings.driveTrashConfirm(n.name) }))) return;
    try {
      await client.driveTrashNode(n.id);
      await load();
    } catch {
      /* ignore */
    }
  }

  async function restore(n: DriveNodeDto) {
    try {
      await client.driveRestoreNode(n.id);
      await load();
    } catch {
      /* ignore */
    }
  }

  async function purge(n: DriveNodeDto) {
    if (!(await confirm({ message: strings.drivePurgeConfirm(n.name), danger: true }))) return;
    try {
      await client.drivePurge(n.id);
      await load();
    } catch {
      /* ignore */
    }
  }

  async function pickedDestination(space: string | null) {
    const target = moveNode;
    setMoveNode(null);
    if (target === null) return;
    try {
      if (target.mode === "move") await client.driveMove(target.id, space, null);
      else await client.driveCopy(target.id, space, null);
      await load();
    } catch {
      /* ignore */
    }
  }

  function rowMenu(n: DriveNodeDto): MenuItem[] {
    if (trashView) {
      return [
        { key: "restore", label: strings.driveRestore, icon: <RotateCcw size={15} />, onClick: () => void restore(n) },
        { key: "purge", label: strings.driveDeleteForever, icon: <Trash2 size={15} />, danger: true, onClick: () => void purge(n) },
      ];
    }
    const items: MenuItem[] = [];
    if (n.kind !== "folder") {
      items.push({ key: "download", label: strings.driveDownload, icon: <Download size={15} />, onClick: () => void download(n) });
      items.push({ key: "versions", label: strings.driveVersionHistory, icon: <History size={15} />, onClick: () => setVersionsNode(n.id) });
    }
    if (canWrite) {
      items.push({ key: "rename", label: strings.driveRename, icon: <Pencil size={15} />, onClick: () => void rename(n) });
      items.push({ key: "move", label: strings.driveMove, icon: <MoveRight size={15} />, onClick: () => setMoveNode({ id: n.id, mode: "move" }) });
      items.push({ key: "copy", label: strings.driveCopy, icon: <Copy size={15} />, onClick: () => setMoveNode({ id: n.id, mode: "copy" }) });
      items.push({ key: "trash", label: strings.driveTrashAction, icon: <Trash2 size={15} />, danger: true, onClick: () => void trash(n) });
    }
    return items;
  }

  return (
    <div className={styles.drive}>
      <aside className={styles.sidebar}>
        <div className={styles.sideGroup}>
          <button
            type="button"
            className={location === null && !trashView ? `${styles.sideItem} ${styles.sideActive}` : styles.sideItem}
            onClick={() => selectLocation(null)}
          >
            <HardDrive size={18} />
            <span>{strings.driveMyFiles}</span>
          </button>
        </div>

        <div className={styles.sideGroup}>
          <div className={styles.sideLabel}>
            {strings.driveSpaces}
            <button type="button" className={styles.sideAdd} onClick={() => void newSpace()} aria-label={strings.driveNewSpace}>
              <Plus size={14} />
            </button>
          </div>
          {spaces.filter((s) => !s.archived).map((s) => (
            <button
              key={s.id}
              type="button"
              className={location === s.id && !trashView ? `${styles.sideItem} ${styles.sideActive}` : styles.sideItem}
              onClick={() => selectLocation(s.id)}
            >
              <Users size={18} />
              <span className={styles.sideName}>{s.name}</span>
            </button>
          ))}
        </div>

        <div className={`${styles.sideGroup} ${styles.sideBottom}`}>
          <button
            type="button"
            className={trashView ? `${styles.sideItem} ${styles.sideActive}` : styles.sideItem}
            onClick={() => {
              setTrashView(true);
              setPath([]);
            }}
          >
            <Trash2 size={18} />
            <span>{strings.driveTrash}</span>
          </button>
        </div>
      </aside>

      <section
        className={dragOver ? `${styles.main} ${styles.mainDrag}` : styles.main}
        onDragOver={(e) => {
          if (canWrite && !trashView) {
            e.preventDefault();
            setDragOver(true);
          }
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={(e) => {
          setDragOver(false);
          if (canWrite && !trashView && e.dataTransfer.files.length > 0) {
            e.preventDefault();
            void uploadFiles(e.dataTransfer.files);
          }
        }}
      >
        <header className={styles.head}>
          <nav className={styles.crumbs}>
            <button type="button" className={styles.crumb} onClick={() => setPath([])}>
              {trashView ? strings.driveTrash : currentSpace?.name ?? strings.driveMyFiles}
            </button>
            {path.map((c, i) => (
              <span key={c.id} className={styles.crumbSep}>
                <ChevronRight size={14} />
                <button type="button" className={styles.crumb} onClick={() => setPath(path.slice(0, i + 1))}>
                  {c.name}
                </button>
              </span>
            ))}
          </nav>
          <div className={styles.actions}>
            {currentSpace !== null && !trashView && (
              <button type="button" className={styles.ghostBtn} onClick={() => setShowMembers(true)}>
                <Users size={15} /> {strings.driveMembers}
              </button>
            )}
            {canWrite && !trashView && (
              <>
                <Menu
                  triggerLabel={strings.driveNew}
                  label={strings.driveNew}
                  icon={<Plus size={15} />}
                  align="end"
                  items={[
                    { key: "doc", label: strings.driveKindDoc, icon: <FileText size={15} />, onClick: () => void newDoc() },
                    { key: "sheet", label: strings.driveKindSheet, icon: <Sheet size={15} />, onClick: () => void newSheet() },
                    // Spreadsheets are alo Sheet only now (ADR 0033): "Sheet"
                    // creates one, and it exports to .xlsx. Word/Slides stay on
                    // Collabora until their native stages land.
                    { key: "word", label: strings.driveKindWord, icon: <FileType size={15} />, onClick: () => void newOffice("docx"), divider: true },
                    { key: "slides", label: strings.driveKindSlides, icon: <Presentation size={15} />, onClick: () => void newOffice("pptx") },
                    { key: "folder", label: strings.driveKindFolder, icon: <FolderPlus size={15} />, onClick: () => void newFolder(), divider: true },
                  ]}
                />
                <button type="button" className={styles.primaryBtn} onClick={() => fileRef.current?.click()} disabled={uploading}>
                  <Upload size={15} /> {uploading ? strings.driveUploading : strings.driveUpload}
                </button>
                <input
                  ref={fileRef}
                  type="file"
                  multiple
                  style={{ display: "none" }}
                  onChange={(e) => {
                    if (e.target.files && e.target.files.length > 0) void uploadFiles(e.target.files);
                    e.target.value = "";
                  }}
                />
              </>
            )}
          </div>
        </header>

        {nodes === null ? (
          <div className={styles.center}>
            <Spinner size={22} />
          </div>
        ) : nodes.length === 0 ? (
          <div className={styles.empty}>
            {trashView ? strings.driveEmptyTrash : strings.driveEmpty}
          </div>
        ) : (
          <ul className={styles.list}>
            <li className={styles.listHead}>
              <span className={styles.colName}>{strings.driveColName}</span>
              <span className={styles.colSize}>{strings.driveColSize}</span>
              <span className={styles.colDate}>{strings.driveColModified}</span>
              <span className={styles.colMenu} />
            </li>
            {nodes.map((n) => {
              const Icon = nodeIcon(n);
              return (
                <li key={n.id} className={styles.row}>
                  <button
                    type="button"
                    className={styles.rowMain}
                    onClick={() => openNode(n)}
                    onDoubleClick={() => openNode(n)}
                  >
                    <Icon size={18} className={n.kind === "folder" ? styles.folderIcon : styles.fileIcon} />
                    <span className={styles.rowName}>{n.name}</span>
                  </button>
                  <span className={styles.colSize}>{n.kind === "folder" ? "—" : fileSize(n.size)}</span>
                  <span className={styles.colDate}>{new Date(n.updatedAt).toLocaleDateString()}</span>
                  <span className={styles.colMenu}>
                    <Menu label={strings.driveActions} icon={<span aria-hidden>⋯</span>} items={rowMenu(n)} />
                  </span>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {moveNode !== null && (
        <DestinationDialog
          spaces={spaces}
          mode={moveNode.mode}
          onPick={(s) => void pickedDestination(s)}
          onClose={() => setMoveNode(null)}
        />
      )}
      {versionsNode !== null && (
        <VersionsDialog nodeId={versionsNode} onChanged={() => void load()} onClose={() => setVersionsNode(null)} />
      )}
      {showMembers && currentSpace !== null && (
        <MembersDialog space={currentSpace} onClose={() => setShowMembers(false)} />
      )}
      {openDoc !== null && (
        <Suspense fallback={null}>
          <DocEditor
            nodeId={openDoc.id}
            name={openDoc.name}
            onClose={() => {
              setOpenDoc(null);
              void load();
            }}
          />
        </Suspense>
      )}
      {openSheet !== null && (
        <Suspense fallback={null}>
          <SheetEditor
            nodeId={openSheet.id}
            name={openSheet.name}
            onClose={() => {
              setOpenSheet(null);
              void load();
            }}
          />
        </Suspense>
      )}
      {openOffice !== null && (
        <Suspense fallback={null}>
          <OfficeEditor
            nodeId={openOffice.id}
            name={openOffice.name}
            onClose={() => {
              setOpenOffice(null);
              void load();
            }}
          />
        </Suspense>
      )}
      {importing !== null && (
        <div className={styles.importOverlay}>
          <Spinner size={24} />
          <div className={styles.importTitle}>{strings.driveImporting(importing)}</div>
          <div className={styles.importNote}>{strings.driveImportNote}</div>
        </div>
      )}
      {importFailed !== null && (
        <div className={styles.importToast} role="alert">
          <span>{strings.driveImportFailed(importFailed)}</span>
          <button
            type="button"
            className={styles.importToastClose}
            onClick={() => setImportFailed(null)}
            aria-label={strings.close}
          >
            <X size={16} />
          </button>
        </div>
      )}
    </div>
  );
}
