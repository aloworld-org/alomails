// alo Drive — the file manager. Left: locations (My Files + the Spaces you
// belong to) and Trash. Right: the current folder's contents with a breadcrumb
// and per-item actions. Every file lives in one location; its access is that
// location's access (ADR 0027), so there is no per-file sharing here — sharing
// is membership of the Space it lives in, always visible via "Members".
import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import {
  ArrowUpDown,
  AlignJustify,
  Check,
  ChevronRight,
  Copy,
  Download,
  FileText,
  FolderOpen,
  HardDrive,
  History,
  Grid2X2,
  Info,
  List,
  MoveRight,
  Pencil,
  Plus,
  RotateCcw,
  Rows3,
  Trash2,
  Upload,
  Users,
  X,
} from "lucide-react";

import { RecordAgentPanel, type RecordOrigin } from "../agents";
import { getLocale, strings } from "../i18n";
import { useJmapClient, type DriveNodeDto, type SpaceDto } from "../jmap";
import { Menu, Spinner, useDialogs, type MenuItem } from "../ds";
import { DestinationDialog, MembersDialog, VersionsDialog } from "./dialogs";
import { DriveCreateActions } from "./DriveCreateActions";
import { blankOfficeFile, type OfficeExt } from "./blankTemplates";
import { nextUntitledName } from "./driveCreation";
// BlockNote is heavy and only needed when a doc opens — code-split it out.
const DocEditor = lazy(() => import("./DocEditor").then((m) => ({ default: m.DocEditor })));
// Univer is heavy; the native Sheet editor only loads when a sheet is opened.
const loadSheetEditor = () => import("./SheetEditor").then((m) => ({ default: m.SheetEditor }));
const SheetEditor = lazy(loadSheetEditor);
const OfficeEditor = lazy(() => import("./OfficeEditor").then((m) => ({ default: m.OfficeEditor })));

/** Real Office files open in Collabora; kept here so it doesn't pull the editor
 *  into the main bundle. */
const OFFICE_EXT = /\.(docx?|xlsx?|pptx?|odt|ods|odp|rtf|csv)$/i;
// Spreadsheets we import natively into alo Sheet instead of opening in Collabora
// (ADR 0033, stage 1). `.xlsx`/`.xlsm` are OOXML; `.xls` (old binary) and `.ods`
// are not covered yet and still fall through to the Office path.
const SPREADSHEET_IMPORT = /\.xls[mx]$/i;
import { driveErrorReason, driveNodeOrigin, fileSize, nodeIcon, saveBlob } from "./parts";
import { xlsxToUniverSnapshot } from "./importOffice";
import styles from "./DriveModule.module.css";

type Crumb = { id: string; name: string };
/** The file an editor is open on: what to call it, and where it came from. */
type OpenEditor = { id: string; name: string; origin: RecordOrigin | null };
type EditorKind = "doc" | "sheet" | "office";
type SortMode = "name-asc" | "name-desc" | "newest" | "oldest" | "largest" | "smallest";
type ViewMode = "extra-large" | "large" | "medium" | "small" | "list" | "details" | "tiles" | "content";
type DriveNotice = {
  kind: "error" | "success";
  message: string;
  action: { label: string; run: () => void } | null;
};

function fileSlug(name: string): string {
  const slug = name
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "untitled";
}

const EDITOR_ROUTE_KEY = "alo.drive.editor-route";

function editorPath(kind: EditorKind, id: string, name: string): string {
  const slug = fileSlug(name);
  try {
    window.localStorage.setItem(`${EDITOR_ROUTE_KEY}:${kind}:${slug}`, id);
  } catch {
    // A clean URL still works for this navigation even when storage is blocked.
  }
  return `/drive/${kind}/${slug}`;
}

function storedEditorId(kind: EditorKind, slug: string): string | null {
  try {
    return window.localStorage.getItem(`${EDITOR_ROUTE_KEY}:${kind}:${slug}`);
  } catch {
    return null;
  }
}

export function DriveModule() {
  const client = useJmapClient();
  const { prompt, confirm } = useDialogs();
  const route = useLocation();
  const navigate = useNavigate();

  const [spaces, setSpaces] = useState<SpaceDto[]>([]);
  const [location, setLocation] = useState<string | null>(null); // null = My Files
  const [trashView, setTrashView] = useState(false);
  const [path, setPath] = useState<Crumb[]>([]);
  const [nodes, setNodes] = useState<DriveNodeDto[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [expandedFolders, setExpandedFolders] = useState<ReadonlySet<string>>(new Set());
  const [folderChildren, setFolderChildren] = useState<ReadonlyMap<string, DriveNodeDto[] | null>>(new Map());
  const [folderErrors, setFolderErrors] = useState<ReadonlyMap<string, string>>(new Map());
  const [spacesAttempt, setSpacesAttempt] = useState(0);
  const [uploading, setUploading] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [sortMode, setSortMode] = useState<SortMode>("name-asc");
  const [viewMode, setViewMode] = useState<ViewMode>("details");
  const [compactView, setCompactView] = useState(false);
  const [navigationPane, setNavigationPane] = useState(true);
  const [showExtensions, setShowExtensions] = useState(true);

  const [moveNodes, setMoveNodes] = useState<{ nodes: DriveNodeDto[]; mode: "move" | "copy" } | null>(null);
  const [selectedNodes, setSelectedNodes] = useState<ReadonlyMap<string, DriveNodeDto>>(new Map());
  const [versionsNode, setVersionsNode] = useState<string | null>(null);
  // The open editor carries the node's origin beside its name, so the record
  // agent panel inside it needs no second read of a node the list already had.
  const [openDoc, setOpenDoc] = useState<OpenEditor | null>(null);
  const [openSheet, setOpenSheet] = useState<OpenEditor | null>(null);
  const [openOffice, setOpenOffice] = useState<OpenEditor | null>(null);
  const [editorRoutePending, setEditorRoutePending] = useState(false);
  const [editorRouteError, setEditorRouteError] = useState<string | null>(null);
  const [editorRouteAttempt, setEditorRouteAttempt] = useState(0);
  // Best-effort import of a real Office file into a native editor (ADR 0033).
  const [importing, setImporting] = useState<string | null>(null);
  const [importFailed, setImportFailed] = useState<string | null>(null);
  const [showMembers, setShowMembers] = useState(false);
  const [notice, setNotice] = useState<DriveNotice | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const uploadTargetRef = useRef<DriveNodeDto | null>(null);

  const parent = path.length > 0 ? (path[path.length - 1]?.id ?? null) : null;
  const currentSpace = useMemo(() => spaces.find((s) => s.id === location) ?? null, [spaces, location]);
  const canWrite = location === null || (currentSpace !== null && currentSpace.myRole !== "viewer");
  const editorRouteActive = /^\/drive\/(doc|sheet|office)\//.test(route.pathname);
  const selected = useMemo(() => Array.from(selectedNodes.values()), [selectedNodes]);
  // The record in focus: one selected item, which is what the details pane is
  // about. Two selections are a bulk action, not a record; the Trash is where
  // a person undoes things, not where they ask an agent about one.
  const focused = selected.length === 1 && !trashView ? (selected[0] as DriveNodeDto) : null;

  const showEditor = useCallback((
    kind: EditorKind,
    id: string,
    name: string,
    origin: RecordOrigin | null = null,
    replace = false,
  ) => {
    const value = { id, name, origin };
    setOpenDoc(kind === "doc" ? value : null);
    setOpenSheet(kind === "sheet" ? value : null);
    setOpenOffice(kind === "office" ? value : null);
    navigate(editorPath(kind, id, name), { replace });
  }, [navigate]);

  // Editor state is URL-backed. A direct visit or browser refresh restores the
  // exact Drive file instead of falling back to the file list.
  useEffect(() => {
    let cancelled = false;
    const legacyMatch = /^\/drive\/(doc|sheet|office)\/([^/]+)\/([^/]*)$/.exec(route.pathname);
    const cleanMatch = /^\/drive\/(doc|sheet|office)\/([^/]+)$/.exec(route.pathname);
    const match = legacyMatch ?? cleanMatch;
    if (match === null) {
      setOpenDoc(null);
      setOpenSheet(null);
      setOpenOffice(null);
      setEditorRoutePending(false);
      setEditorRouteError(null);
      return undefined;
    }
    const kind = match[1] as EditorKind;
    const routeValue = decodeURIComponent(match[2] ?? "");
    const id = legacyMatch !== null ? routeValue : (storedEditorId(kind, routeValue) ?? "");
    if (id === "") {
      setEditorRoutePending(false);
      setEditorRouteError(strings.driveFileUnavailable);
      return undefined;
    }
    setEditorRoutePending(true);
    setEditorRouteError(null);
    void client.driveNode(id).then((node) => {
      if (cancelled) return;
      setEditorRoutePending(false);
      if (node === null) {
        setEditorRouteError(strings.driveFileUnavailable);
        return;
      }
      const canonicalKind: EditorKind = node.kind === "doc" ? "doc" : node.kind === "sheet" ? "sheet" : "office";
      const canonicalPath = editorPath(canonicalKind, node.id, node.name);
      const value = { id: node.id, name: node.name, origin: driveNodeOrigin(node) };
      setOpenDoc(canonicalKind === "doc" ? value : null);
      setOpenSheet(canonicalKind === "sheet" ? value : null);
      setOpenOffice(canonicalKind === "office" ? value : null);
      if (route.pathname !== canonicalPath) navigate(canonicalPath, { replace: true });
    }).catch((error: unknown) => {
      if (cancelled) return;
      setEditorRoutePending(false);
      const reason = driveErrorReason(error) ?? strings.driveUnknownError;
      setEditorRouteError(strings.driveEditorLoadFailed(reason));
    });
    return () => { cancelled = true; };
  }, [client, editorRouteAttempt, navigate, route.pathname]);

  const loadSpaces = useCallback(() => {
    void client.spaces().then(setSpaces).catch((error: unknown) => {
      const reason = driveErrorReason(error) ?? strings.driveUnknownError;
      setSpaces([]);
      setNotice({
        kind: "error",
        message: strings.driveSpacesLoadFailed(reason),
        action: { label: strings.driveRetry, run: () => setSpacesAttempt((value) => value + 1) },
      });
    });
  }, [client]);

  const load = useCallback(async () => {
    setExpandedFolders(new Set());
    setFolderChildren(new Map());
    setFolderErrors(new Map());
    setSelectedNodes(new Map());
    setLoadError(null);
    try {
      const list = trashView
        ? await client.driveTrash(location)
        : await client.driveList(location, parent);
      setNodes(list);
    } catch (error) {
      setNodes([]);
      setLoadError(driveErrorReason(error) ?? strings.driveUnknownError);
    }
  }, [client, location, parent, trashView]);

  useEffect(() => {
    if (!editorRouteActive) loadSpaces();
  }, [editorRouteActive, loadSpaces, spacesAttempt]);
  useEffect(() => {
    if (editorRouteActive) return;
    setNodes(null);
    void load();
  }, [editorRouteActive, load]);

  // Univer is the largest Drive editor bundle. Fetch it after the file list is
  // usable, so a subsequent sheet click does not start from a cold download.
  useEffect(() => {
    if (nodes === null || editorRouteActive) return undefined;
    const timer = window.setTimeout(() => void loadSheetEditor(), 600);
    return () => window.clearTimeout(timer);
  }, [editorRouteActive, nodes]);

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
      else if (node.kind === "doc") showEditor("doc", id, node.name, driveNodeOrigin(node));
      else if (node.kind === "sheet") showEditor("sheet", id, node.name, driveNodeOrigin(node));
      else if (node.kind === "file" && SPREADSHEET_IMPORT.test(node.name))
        void importSpreadsheet(id, node.name);
      else if (node.kind === "file" && OFFICE_EXT.test(node.name))
        showEditor("office", id, node.name, driveNodeOrigin(node));
    });
  }, [searchParams, setSearchParams, client, showEditor]);

  function selectLocation(space: string | null) {
    setLocation(space);
    setTrashView(false);
    setPath([]);
  }

  function openNode(n: DriveNodeDto) {
    if (n.kind === "doc") showEditor("doc", n.id, n.name, driveNodeOrigin(n));
    else if (n.kind === "sheet") showEditor("sheet", n.id, n.name, driveNodeOrigin(n));
    else if (n.kind === "file" && SPREADSHEET_IMPORT.test(n.name)) void importSpreadsheet(n.id, n.name);
    else if (n.kind === "file" && OFFICE_EXT.test(n.name)) showEditor("office", n.id, n.name, driveNodeOrigin(n));
    else void download(n);
  }

  function loadFolderChildren(folder: DriveNodeDto) {
    setFolderErrors((current) => {
      const next = new Map(current);
      next.delete(folder.id);
      return next;
    });
    setFolderChildren((current) => new Map(current).set(folder.id, null));
    void client.driveList(location, folder.id).then((children) => {
      setFolderChildren((current) => new Map(current).set(folder.id, children));
    }).catch((error: unknown) => {
      setFolderChildren((current) => new Map(current).set(folder.id, []));
      setFolderErrors((current) => new Map(current).set(folder.id, driveErrorReason(error) ?? strings.driveUnknownError));
    });
  }

  function toggleFolder(folder: DriveNodeDto) {
    if (trashView) return;
    const isExpanded = expandedFolders.has(folder.id);
    setExpandedFolders((current) => {
      const next = new Set(current);
      if (isExpanded) next.delete(folder.id);
      else next.add(folder.id);
      return next;
    });
    if (isExpanded || folderChildren.has(folder.id)) return;
    loadFolderChildren(folder);
  }

  function sortNodes(items: DriveNodeDto[]): DriveNodeDto[] {
    return [...items].sort((a, b) => {
      if (a.kind === "folder" && b.kind !== "folder") return -1;
      if (a.kind !== "folder" && b.kind === "folder") return 1;
      if (sortMode === "name-asc") return a.name.localeCompare(b.name);
      if (sortMode === "name-desc") return b.name.localeCompare(a.name);
      if (sortMode === "newest") return new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime();
      if (sortMode === "oldest") return new Date(a.updatedAt).getTime() - new Date(b.updatedAt).getTime();
      if (sortMode === "largest") return b.size - a.size;
      return a.size - b.size;
    });
  }

  function displayName(node: DriveNodeDto): string {
    if (showExtensions || node.kind === "folder") return node.name;
    const dot = node.name.lastIndexOf(".");
    return dot > 0 ? node.name.slice(0, dot) : node.name;
  }

  function reportFailure(action: string, error: unknown) {
    const reason = driveErrorReason(error) ?? strings.driveUnknownError;
    setNotice({ kind: "error", message: strings.driveActionFailed(action, reason), action: null });
  }

  function toggleSelected(node: DriveNodeDto, checked: boolean) {
    setSelectedNodes((current) => {
      const next = new Map(current);
      if (checked) next.set(node.id, node);
      else next.delete(node.id);
      return next;
    });
  }

  function selectableNodes(items: DriveNodeDto[]): DriveNodeDto[] {
    const result: DriveNodeDto[] = [];
    for (const node of items) {
      result.push(node);
      if (expandedFolders.has(node.id)) {
        const children = folderChildren.get(node.id);
        if (children) result.push(...selectableNodes(children));
      }
    }
    return result;
  }

  function renderRows(items: DriveNodeDto[], depth = 0): ReactNode[] {
    return sortNodes(items).map((n) => {
      const Icon = nodeIcon(n);
      const folder = n.kind === "folder";
      const expanded = folder && expandedFolders.has(n.id);
      const children = folderChildren.get(n.id);
      const folderError = folderErrors.get(n.id);
      const row = (
        <div
          className={`${styles.row} ${selectedNodes.has(n.id) ? styles.rowSelected : ""} ${depth > 0 ? styles.nestedRow : ""}`}
          style={{ "--drive-depth": depth } as CSSProperties}
        >
          <label className={styles.rowSelect}>
            <input
              type="checkbox"
              checked={selectedNodes.has(n.id)}
              onChange={(event) => toggleSelected(n, event.target.checked)}
              aria-label={strings.driveSelectItem(n.name)}
            />
          </label>
          <button
            type="button"
            className={styles.rowMain}
            onPointerEnter={() => {
              if (n.kind === "sheet") void loadSheetEditor();
            }}
            onFocus={() => {
              if (n.kind === "sheet") void loadSheetEditor();
            }}
            onClick={() => folder ? toggleFolder(n) : openNode(n)}
            aria-expanded={folder ? expanded : undefined}
          >
            {folder && (
              <ChevronRight
                size={15}
                className={`${styles.folderChevron} ${expanded ? styles.folderChevronOpen : ""}`}
              />
            )}
            <Icon size={18} className={folder ? styles.folderIcon : styles.fileIcon} />
            <span className={styles.rowName}>{displayName(n)}</span>
          </button>
          <span className={styles.colSize}>{folder ? "—" : fileSize(n.size)}</span>
          <span className={styles.colDate}>{new Date(n.updatedAt).toLocaleDateString()}</span>
          <span className={styles.colMenu}>
            <Menu
              label={strings.driveActions}
              icon={<span aria-hidden>⋯</span>}
              items={rowMenu(n)}
              align={viewMode === "details" || viewMode === "content" ? "end" : "start"}
            />
          </span>
        </div>
      );
      return (
        <li key={n.id} className={expanded ? styles.folderGroup : styles.nodeItem}>
          {row}
          {expanded && (
            <ul className={styles.folderChildren}>
              {folderError !== undefined ? (
                <li className={`${styles.row} ${styles.nestedStatus}`}>
                  <span className={styles.nestedStatusContent} role="alert">
                    {strings.driveFolderLoadFailed(folderError)}
                    <button type="button" onClick={() => loadFolderChildren(n)}>{strings.driveRetry}</button>
                  </span>
                </li>
              ) : children === null || children === undefined ? (
                <li className={`${styles.row} ${styles.nestedStatus}`}>
                  <span className={styles.nestedStatusContent} role="status" aria-label={strings.driveFolderLoading(n.name)}>
                    <span className={styles.nestedSkeleton} />
                    <span className={styles.nestedSkeleton} />
                    <span className={styles.nestedSkeletonShort} />
                  </span>
                </li>
              ) : children.length === 0 ? (
                <li className={`${styles.row} ${styles.nestedStatus}`}>
                  <span className={styles.nestedStatusContent}>
                    {strings.driveFolderEmpty}
                    {canWrite && (
                      <button type="button" onClick={() => chooseUpload(n)}>
                        <Upload size={15} /> {strings.driveUploadHere}
                      </button>
                    )}
                  </span>
                </li>
              ) : renderRows(children, depth + 1)}
            </ul>
          )}
        </li>
      );
    });
  }

  async function newDoc() {
    const name = nextUntitledName(
      strings.docsUntitled,
      nodes?.map((node) => node.name) ?? [],
    );
    try {
      const id = await client.driveCreateDoc(location, parent, name);
      await load();
      showEditor("doc", id, name);
    } catch (error) {
      reportFailure(strings.driveKindDoc, error);
    }
  }

  async function newSheet() {
    const name = (await prompt({ message: strings.driveNewSheetPrompt }))?.trim();
    if (!name) return;
    try {
      const id = await client.driveCreateSheet(location, parent, name);
      await load();
      showEditor("sheet", id, name);
    } catch (error) {
      reportFailure(strings.driveKindSheet, error);
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
      showEditor("sheet", id, base);
    } catch (error) {
      setImportFailed(fileName);
      reportFailure(strings.driveImporting(fileName), error);
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
      showEditor("office", id, file.name);
    } catch (error) {
      reportFailure(kind, error);
    }
  }

  async function download(n: DriveNodeDto) {
    if (n.blobId === null) return;
    try {
      saveBlob(await client.driveDownload(n.id), n.name);
    } catch (error) {
      reportFailure(strings.driveDownload, error);
    }
  }

  function chooseUpload(folder: DriveNodeDto | null = null) {
    uploadTargetRef.current = folder;
    fileRef.current?.click();
  }

  async function uploadFiles(files: FileList | File[], targetFolder: DriveNodeDto | null = null) {
    setUploading(true);
    try {
      for (const f of Array.from(files)) {
        await client.driveUpload(location, targetFolder?.id ?? parent, f);
      }
      if (targetFolder === null) await load();
      else loadFolderChildren(targetFolder);
    } catch (error) {
      reportFailure(strings.driveUpload, error);
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
    } catch (error) {
      reportFailure(strings.driveNewFolder, error);
    }
  }

  async function newSpace() {
    const name = (await prompt({ message: strings.driveNewSpacePrompt }))?.trim();
    if (!name) return;
    try {
      const id = await client.createSpace(name);
      loadSpaces();
      selectLocation(id);
    } catch (error) {
      reportFailure(strings.driveNewSpace, error);
    }
  }

  async function rename(n: DriveNodeDto) {
    const name = (await prompt({ message: strings.driveRenamePrompt, defaultValue: n.name }))?.trim();
    if (!name || name === n.name) return;
    try {
      await client.driveRename(n.id, name);
      await load();
    } catch (error) {
      reportFailure(strings.driveRename, error);
    }
  }

  async function trashNodes(items: DriveNodeDto[]) {
    if (items.length === 0) return;
    try {
      for (const item of items) await client.driveTrashNode(item.id);
      await load();
      setNotice({
        kind: "success",
        message: items.length === 1 ? strings.driveMovedToTrash(items[0]?.name ?? "") : strings.driveItemsMovedToTrash(items.length),
        action: {
          label: strings.driveUndo,
          run: () => {
            void restoreNodes(items, true);
          },
        },
      });
    } catch (error) {
      reportFailure(strings.driveTrashAction, error);
    }
  }

  async function restoreNodes(items: DriveNodeDto[], fromUndo = false) {
    if (items.length === 0) return;
    try {
      for (const item of items) await client.driveRestoreNode(item.id);
      await load();
      if (fromUndo) {
        setNotice({
          kind: "success",
          message: items.length === 1 ? strings.driveRestoredFromTrash(items[0]?.name ?? "") : strings.driveItemsRestored(items.length),
          action: null,
        });
      }
    } catch (error) {
      reportFailure(strings.driveRestore, error);
    }
  }

  async function purgeNodes(items: DriveNodeDto[]) {
    if (items.length === 0) return;
    const message = items.length === 1
      ? strings.drivePurgeConfirm(items[0]?.name ?? "")
      : strings.drivePurgeManyConfirm(items.length);
    if (!(await confirm({ message, danger: true }))) return;
    try {
      for (const item of items) await client.drivePurge(item.id);
      await load();
    } catch (error) {
      reportFailure(strings.driveDeleteForever, error);
    }
  }

  async function pickedDestination(space: string | null) {
    const target = moveNodes;
    setMoveNodes(null);
    if (target === null) return;
    try {
      for (const node of target.nodes) {
        if (target.mode === "move") await client.driveMove(node.id, space, null);
        else await client.driveCopy(node.id, space, null);
      }
      await load();
    } catch (error) {
      reportFailure(target.mode === "move" ? strings.driveMove : strings.driveCopy, error);
    }
  }

  function rowMenu(n: DriveNodeDto): MenuItem[] {
    if (trashView) {
      return [
        { key: "restore", label: strings.driveRestore, icon: <RotateCcw size={15} />, onClick: () => void restoreNodes([n]) },
        { key: "purge", label: strings.driveDeleteForever, icon: <Trash2 size={15} />, danger: true, onClick: () => void purgeNodes([n]) },
      ];
    }
    const items: MenuItem[] = [
      {
        key: "details",
        label: strings.driveDetailsTitle,
        icon: <Info size={15} />,
        onClick: () => setSelectedNodes(new Map([[n.id, n]])),
      },
    ];
    if (n.kind !== "folder") {
      items.push({ key: "download", label: strings.driveDownload, icon: <Download size={15} />, onClick: () => void download(n) });
      items.push({ key: "versions", label: strings.driveVersionHistory, icon: <History size={15} />, onClick: () => setVersionsNode(n.id) });
    }
    if (canWrite) {
      items.push({ key: "rename", label: strings.driveRename, icon: <Pencil size={15} />, onClick: () => void rename(n) });
      items.push({ key: "move", label: strings.driveMove, icon: <MoveRight size={15} />, onClick: () => setMoveNodes({ nodes: [n], mode: "move" }) });
      items.push({ key: "copy", label: strings.driveCopy, icon: <Copy size={15} />, onClick: () => setMoveNodes({ nodes: [n], mode: "copy" }) });
      items.push({ key: "trash", label: strings.driveTrashAction, icon: <Trash2 size={15} />, danger: true, onClick: () => void trashNodes([n]) });
    }
    return items;
  }

  return (
    <div className={focused === null ? styles.drive : `${styles.drive} ${styles.driveWithDetails}`}>
      <aside className={`${styles.sidebar} ${navigationPane ? "" : styles.sidebarHidden}`}>
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
        <nav className={styles.mobileLocations} aria-label={strings.driveLocations}>
          <button
            type="button"
            className={location === null && !trashView ? styles.mobileLocationActive : styles.mobileLocation}
            onClick={() => selectLocation(null)}
          >
            <HardDrive size={17} /> {strings.driveMyFiles}
          </button>
          {spaces.filter((space) => !space.archived).map((space) => (
            <button
              key={space.id}
              type="button"
              className={location === space.id && !trashView ? styles.mobileLocationActive : styles.mobileLocation}
              onClick={() => selectLocation(space.id)}
            >
              <Users size={17} /> {space.name}
            </button>
          ))}
          <button
            type="button"
            className={trashView ? styles.mobileLocationActive : styles.mobileLocation}
            onClick={() => { setTrashView(true); setPath([]); }}
          >
            <Trash2 size={17} /> {strings.driveTrash}
          </button>
        </nav>
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
            {selected.length > 0 ? (
              <div className={styles.selectionBar} role="toolbar" aria-label={strings.driveSelectionActions}>
                <button type="button" className={styles.selectionClear} onClick={() => setSelectedNodes(new Map())} aria-label={strings.driveClearSelection}>
                  <X size={17} />
                </button>
                <strong>{strings.driveSelected(selected.length)}</strong>
                {trashView ? (
                  <>
                    <button type="button" className={styles.selectionAction} onClick={() => void restoreNodes(selected)}>
                      <RotateCcw size={17} /> {strings.driveRestore}
                    </button>
                    <button type="button" className={`${styles.selectionAction} ${styles.selectionDanger}`} onClick={() => void purgeNodes(selected)}>
                      <Trash2 size={17} /> {strings.driveDeleteForever}
                    </button>
                  </>
                ) : (
                  <>
                    {selected.length === 1 && selected[0]?.kind !== "folder" && (
                      <button type="button" className={styles.selectionAction} onClick={() => void download(selected[0] as DriveNodeDto)}>
                        <Download size={17} /> {strings.driveDownload}
                      </button>
                    )}
                    {canWrite && (
                      <>
                        {selected.length === 1 && (
                          <button type="button" className={styles.selectionAction} onClick={() => void rename(selected[0] as DriveNodeDto)}>
                            <Pencil size={17} /> {strings.driveRename}
                          </button>
                        )}
                        <button type="button" className={styles.selectionAction} onClick={() => setMoveNodes({ nodes: selected, mode: "move" })}>
                          <MoveRight size={17} /> {strings.driveMove}
                        </button>
                        <button type="button" className={styles.selectionAction} onClick={() => setMoveNodes({ nodes: selected, mode: "copy" })}>
                          <Copy size={17} /> {strings.driveCopy}
                        </button>
                        <button type="button" className={`${styles.selectionAction} ${styles.selectionDanger}`} onClick={() => void trashNodes(selected)}>
                          <Trash2 size={17} /> {strings.driveTrashAction}
                        </button>
                      </>
                    )}
                  </>
                )}
              </div>
            ) : !trashView && (
              <>
                <Menu
                  triggerLabel={strings.driveSort}
                  label={strings.driveSort}
                  icon={<ArrowUpDown size={15} />}
                  align="end"
                  items={([
                    ["name-asc", strings.driveSortNameAsc],
                    ["name-desc", strings.driveSortNameDesc],
                    ["newest", strings.driveSortNewest],
                    ["oldest", strings.driveSortOldest],
                    ["largest", strings.driveSortLargest],
                    ["smallest", strings.driveSortSmallest],
                  ] as const).map(([key, label], index) => ({
                    key,
                    label,
                    icon: sortMode === key ? <Check size={15} /> : <span className={styles.menuIconSpace} />,
                    divider: index === 2 || index === 4,
                    onClick: () => setSortMode(key),
                  }))}
                />
                <Menu
                  triggerLabel={strings.driveView}
                  label={strings.driveView}
                  icon={<Rows3 size={15} />}
                  align="end"
                  items={[...([
                    ["extra-large", strings.driveViewExtraLarge, <Grid2X2 size={18} />],
                    ["large", strings.driveViewLarge, <Grid2X2 size={17} />],
                    ["medium", strings.driveViewMedium, <Grid2X2 size={16} />],
                    ["small", strings.driveViewSmall, <Grid2X2 size={14} />],
                    ["list", strings.driveViewList, <List size={15} />],
                    ["details", strings.driveViewDetails, <Rows3 size={15} />],
                    ["tiles", strings.driveViewTiles, <Grid2X2 size={15} />],
                    ["content", strings.driveViewContent, <AlignJustify size={15} />],
                  ] as const).map(([key, label, layoutIcon], index) => ({
                    key,
                    label,
                    icon: viewMode === key ? <Check size={15} /> : layoutIcon,
                    divider: index === 4,
                    onClick: () => setViewMode(key),
                  })),
                    {
                      key: "navigation-pane",
                      label: strings.driveViewNavigationPane,
                      icon: navigationPane ? <Check size={15} /> : <span className={styles.menuIconSpace} />,
                      divider: true,
                      onClick: () => setNavigationPane((visible) => !visible),
                    },
                    {
                      key: "compact-view",
                      label: strings.driveViewCompact,
                      icon: compactView ? <Check size={15} /> : <span className={styles.menuIconSpace} />,
                      onClick: () => setCompactView((compact) => !compact),
                    },
                    {
                      key: "extensions",
                      label: strings.driveViewExtensions,
                      icon: showExtensions ? <Check size={15} /> : <span className={styles.menuIconSpace} />,
                      onClick: () => setShowExtensions((visible) => !visible),
                    },
                  ]}
                />
              </>
            )}
            {currentSpace !== null && !trashView && (
              <button type="button" className={styles.ghostBtn} onClick={() => setShowMembers(true)}>
                <Users size={15} /> {strings.driveMembers}
              </button>
            )}
            {canWrite && !trashView && (
              <>
                <DriveCreateActions
                  labels={{
                    createDocument: strings.driveCreateDocument,
                    aloDocument: strings.driveAloDocument,
                    sheet: strings.driveKindSheet,
                    word: strings.driveKindWord,
                    slides: strings.driveKindSlides,
                    folder: strings.driveNewFolder,
                    upload: uploading ? strings.driveUploading : strings.driveUpload,
                  }}
                  onCreateDocument={() => void newDoc()}
                  onCreateSheet={() => void newSheet()}
                  onCreateWord={() => void newOffice("docx")}
                  onCreateSlides={() => void newOffice("pptx")}
                  onCreateFolder={() => void newFolder()}
                  onUpload={() => chooseUpload()}
                  uploadDisabled={uploading}
                />
                <input
                  ref={fileRef}
                  type="file"
                  multiple
                  hidden
                  onChange={(e) => {
                    const targetFolder = uploadTargetRef.current;
                    uploadTargetRef.current = null;
                    if (e.target.files && e.target.files.length > 0) void uploadFiles(e.target.files, targetFolder);
                    e.target.value = "";
                  }}
                />
              </>
            )}
          </div>
        </header>

        {nodes === null ? (
          <DriveSkeleton />
        ) : loadError !== null ? (
          <div className={styles.loadError} role="alert">
            <FolderOpen size={38} />
            <h2>{strings.driveLoadFailedTitle}</h2>
            <p>{strings.driveLoadFailed(loadError)}</p>
            <button type="button" className={styles.emptyPrimary} onClick={() => void load()}>
              {strings.driveRetry}
            </button>
          </div>
        ) : nodes.length === 0 ? (
          <div className={styles.emptyState}>
            <span className={styles.emptyArt}>
              {trashView ? <Trash2 size={38} /> : <FolderOpen size={38} />}
            </span>
            <h2 className={styles.emptyTitle}>
              {trashView ? strings.driveEmptyTrashTitle : strings.driveEmptyTitle}
            </h2>
            <p className={styles.emptyBody}>
              {trashView ? strings.driveEmptyTrash : canWrite ? strings.driveEmpty : strings.driveEmptyReadOnly}
            </p>
            {canWrite && !trashView && (
              <div className={styles.emptyActions}>
                <DriveCreateActions
                  labels={{
                    createDocument: strings.driveCreateDocument,
                    aloDocument: strings.driveAloDocument,
                    sheet: strings.driveKindSheet,
                    word: strings.driveKindWord,
                    slides: strings.driveKindSlides,
                    folder: strings.driveNewFolder,
                    upload: uploading ? strings.driveUploading : strings.driveUpload,
                  }}
                  onCreateDocument={() => void newDoc()}
                  onCreateSheet={() => void newSheet()}
                  onCreateWord={() => void newOffice("docx")}
                  onCreateSlides={() => void newOffice("pptx")}
                  onCreateFolder={() => void newFolder()}
                  onUpload={() => chooseUpload()}
                  uploadDisabled={uploading}
                  align="start"
                />
              </div>
            )}
          </div>
        ) : (
          <ul className={`${styles.list} ${styles[`view_${viewMode}`] ?? ""} ${compactView ? styles.listCompact : ""}`}>
            <li className={styles.listHead}>
              <label className={styles.selectAll}>
                <input
                  type="checkbox"
                  checked={selectableNodes(nodes).length > 0 && selected.length === selectableNodes(nodes).length}
                  onChange={(event) => {
                    const visible = selectableNodes(nodes);
                    setSelectedNodes(event.target.checked ? new Map(visible.map((node) => [node.id, node])) : new Map());
                  }}
                  aria-label={strings.driveSelectAll}
                />
              </label>
              <span className={styles.colName}>{strings.driveColName}</span>
              <span className={styles.colSize}>{strings.driveColSize}</span>
              <span className={styles.colDate}>{strings.driveColModified}</span>
              <span className={styles.colMenu} />
            </li>
            {renderRows(nodes)}
          </ul>
        )}
      </section>

      {focused !== null && <DetailsPane node={focused} />}

      {moveNodes !== null && (
        <DestinationDialog
          spaces={spaces}
          mode={moveNodes.mode}
          onPick={(s) => void pickedDestination(s)}
          onClose={() => setMoveNodes(null)}
        />
      )}
      {versionsNode !== null && (
        <VersionsDialog nodeId={versionsNode} onChanged={() => void load()} onClose={() => setVersionsNode(null)} />
      )}
      {showMembers && currentSpace !== null && (
        <MembersDialog space={currentSpace} onClose={() => setShowMembers(false)} />
      )}
      {editorRoutePending && openDoc === null && openSheet === null && openOffice === null && (
        <EditorLoading name={strings.driveOpeningEditor} />
      )}
      {editorRouteError !== null && (
        <div className={styles.editorRouteError} role="alert">
          <div className={styles.loadError}>
            <FileText size={38} />
            <h2>{strings.driveFileOpenFailedTitle}</h2>
            <p>{editorRouteError}</p>
            <div className={styles.emptyActions}>
              <button type="button" className={styles.emptyPrimary} onClick={() => setEditorRouteAttempt((value) => value + 1)}>
                {strings.driveRetry}
              </button>
              <button type="button" className={styles.emptySecondary} onClick={() => navigate("/drive", { replace: true })}>
                {strings.driveBackToFiles}
              </button>
            </div>
          </div>
        </div>
      )}
      {openDoc !== null && (
        <Suspense fallback={<EditorLoading name={openDoc.name} />}>
          <DocEditor
            nodeId={openDoc.id}
            name={openDoc.name}
            origin={openDoc.origin}
            onClose={() => {
              navigate("/drive", { replace: true });
              void load();
            }}
          />
        </Suspense>
      )}
      {openSheet !== null && (
        <Suspense fallback={<EditorLoading name={openSheet.name} />}>
          <SheetEditor
            nodeId={openSheet.id}
            name={openSheet.name}
            origin={openSheet.origin}
            onNameChange={(nextName) => showEditor("sheet", openSheet.id, nextName, openSheet.origin, true)}
            onClose={() => {
              navigate("/drive", { replace: true });
              void load();
            }}
          />
        </Suspense>
      )}
      {openOffice !== null && (
        <Suspense fallback={<EditorLoading name={openOffice.name} />}>
          <OfficeEditor
            nodeId={openOffice.id}
            name={openOffice.name}
            onClose={() => {
              navigate("/drive", { replace: true });
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
      {notice !== null && (
        <div className={`${styles.driveNotice} ${notice.kind === "error" ? styles.driveNoticeError : ""}`} role={notice.kind === "error" ? "alert" : "status"}>
          <span>{notice.message}</span>
          {notice.action !== null && (
            <button type="button" className={styles.driveNoticeAction} onClick={() => {
              const run = notice.action?.run;
              setNotice(null);
              run?.();
            }}>
              {notice.action.label}
            </button>
          )}
          <button type="button" className={styles.driveNoticeClose} onClick={() => setNotice(null)} aria-label={strings.close}>
            <X size={16} />
          </button>
        </div>
      )}
    </div>
  );
}

/** The selected item's own pane: what it is, and its agent (A8.4) — where the
 *  file came from, what @drive can do with it, and a question about it
 *  answered in place. A document or a sheet has an agent of its own inside
 *  its editor; here the record is a file in a file manager, so the agent is
 *  the file manager's. */
function DetailsPane({ node }: { node: DriveNodeDto }) {
  const Icon = nodeIcon(node);
  const folder = node.kind === "folder";
  return (
    <aside className={styles.details} aria-label={strings.driveDetailsTitle}>
      <header className="flex min-w-0 items-center gap-3">
        <Icon size={22} className="shrink-0 text-tertiary" />
        <span className="flex min-w-0 flex-col">
          <span className="truncate text-sm font-semibold text-primary" title={node.name}>
            {node.name}
          </span>
          <span className="text-xs text-tertiary">{strings.driveDetailsTitle}</span>
        </span>
      </header>
      <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-xs">
        <dt className="text-tertiary">{strings.driveColSize}</dt>
        <dd className="m-0 text-secondary">{folder ? "—" : fileSize(node.size)}</dd>
        <dt className="text-tertiary">{strings.driveColModified}</dt>
        <dd className="m-0 text-secondary">
          {new Date(node.updatedAt).toLocaleString(getLocale())}
        </dd>
      </dl>
      <RecordAgentPanel
        product="drive"
        recordKind={node.kind}
        recordId={node.id}
        recordLabel={node.name}
        origin={driveNodeOrigin(node)}
      />
    </aside>
  );
}

function EditorLoading({ name }: { name: string }) {
  return (
    <div className={styles.editorLoading} role="status" aria-label={strings.driveLoadingFile(name)} aria-busy="true">
      <div className={styles.editorLoadingHead}>
        <span className={styles.editorLoadingIcon} />
        <span className={styles.editorLoadingName}>{name}</span>
      </div>
      <div className={styles.editorLoadingToolbar}>
        {Array.from({ length: 8 }, (_, index) => <span key={index} />)}
      </div>
      <div className={styles.editorLoadingCanvas}>
        <span /><span /><span /><span />
      </div>
    </div>
  );
}

function DriveSkeleton() {
  return (
    <div className={styles.driveSkeleton} role="status" aria-label={strings.driveLoading} aria-busy="true">
      <span className={styles.driveSkeletonHead} />
      {Array.from({ length: 7 }, (_, index) => (
        <span key={index} className={styles.driveSkeletonRow}>
          <span className={styles.driveSkeletonIcon} />
          <span className={styles.driveSkeletonName} />
          <span className={styles.driveSkeletonMeta} />
        </span>
      ))}
    </div>
  );
}
