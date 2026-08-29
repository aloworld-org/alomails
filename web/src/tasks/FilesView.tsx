// The Files tab: every attachment across a project's tasks, from the real
// /tasks/files roll-up. Click a name to open its task; download pulls the blob
// through the same authenticated blob endpoint mail attachments use.
import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Download, File, FileImage, FileText, HardDrive, Paperclip, Upload } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type DriveNodeDto, type ProjectFileDto, type Task } from "../jmap";
import { Spinner } from "../ds";
import { EmptyState } from "../projects/parts";
import { DriveAttachmentPicker } from "./DriveAttachmentPicker";

function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function FileIcon({ filename }: { filename: string }) {
  const extension = filename.split(".").pop()?.toLowerCase();
  const Icon = extension !== undefined && ["png", "jpg", "jpeg", "gif", "webp", "svg"].includes(extension)
    ? FileImage
    : extension !== undefined && ["pdf", "doc", "docx", "txt", "md"].includes(extension)
      ? FileText
      : File;
  return <Icon size={19} aria-hidden="true" />;
}

interface Props {
  projectId: string;
  onOpen: (taskId: string) => void;
  onCreate: () => void;
}

export function FilesView({ projectId, onOpen, onCreate }: Props) {
  const client = useJmapClient();
  const [files, setFiles] = useState<ProjectFileDto[] | null>(null);
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [taskId, setTaskId] = useState("");
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState(false);
  const [driveOpen, setDriveOpen] = useState(false);
  const [taskMenuOpen, setTaskMenuOpen] = useState(false);
  const [dragging, setDragging] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const taskPickerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let live = true;
    Promise.all([client.projectFiles(projectId), client.tasks(projectId)])
      .then(([nextFiles, nextTasks]) => {
        if (!live) return;
        setFiles(nextFiles);
        setTasks(nextTasks);
        setTaskId((current) => current || nextTasks[0]?.id || "");
      })
      .catch(() => {
        if (!live) return;
        setFiles([]);
        setTasks([]);
      });
    return () => {
      live = false;
    };
  }, [client, projectId]);

  useEffect(() => {
    function closeTaskMenu(event: MouseEvent) {
      if (taskPickerRef.current?.contains(event.target as Node) === false) setTaskMenuOpen(false);
    }
    document.addEventListener("mousedown", closeTaskMenu);
    return () => document.removeEventListener("mousedown", closeTaskMenu);
  }, []);

  async function upload(selected: FileList | File[]) {
    if (taskId === "" || selected.length === 0) return;
    setUploading(true);
    setUploadError(false);
    try {
      for (const file of Array.from(selected)) {
        const { blobId, size } = await client.uploadFile(file);
        await client.addTaskAttachment(taskId, blobId, file.name, size);
      }
      setFiles(await client.projectFiles(projectId));
    } catch {
      setUploadError(true);
    } finally {
      setUploading(false);
      if (fileRef.current !== null) fileRef.current.value = "";
    }
  }

  async function attachDriveFiles(nodes: DriveNodeDto[]) {
    if (taskId === "") return;
    setUploadError(false);
    try {
      for (const node of nodes) {
        if (node.blobId !== null) {
          await client.addTaskAttachment(taskId, node.blobId, node.name, node.size);
        }
      }
      setFiles(await client.projectFiles(projectId));
      setDriveOpen(false);
    } catch {
      setUploadError(true);
    }
  }

  async function download(f: ProjectFileDto) {
    try {
      const blob = await client.downloadTaskAttachment(f.taskId, f.id);
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = f.filename;
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      /* ignore */
    }
  }

  if (files === null || tasks === null) {
    return (
      <div className="grid min-h-80 place-items-center text-tertiary">
        <Spinner size={20} />
      </div>
    );
  }
  if (tasks.length === 0) {
    return (
      <div className="p-6 max-sm:p-4">
        <EmptyState
          Icon={Paperclip}
          title={strings.taskFilesEmpty}
          body={strings.taskFilesNeedTask}
          cta={strings.taskCreateFirst}
          onCta={onCreate}
        />
      </div>
    );
  }

  return (
    <div
      className="mx-auto w-full max-w-[100rem] px-6 pb-8 pt-6 max-sm:px-4"
      onDragEnter={(event) => {
        event.preventDefault();
        setDragging(true);
      }}
      onDragOver={(event) => {
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
      }}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node)) setDragging(false);
      }}
      onDrop={(event) => {
        event.preventDefault();
        setDragging(false);
        void upload(event.dataTransfer.files);
      }}
    >
      <section className="overflow-visible rounded-2xl border border-subtle bg-surface shadow-sm">
        <header className="flex flex-wrap items-center justify-between gap-4 border-b border-subtle px-5 py-4">
          <div className="flex min-w-0 items-center gap-3">
            <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent" aria-hidden="true">
              <Paperclip size={19} />
            </span>
            <div className="min-w-0">
              <h2 className="m-0 text-base font-semibold text-primary">{strings.taskFiles}</h2>
              <p className="m-0 mt-0.5 text-xs text-tertiary">{strings.taskSummaryTotal(files.length)}</p>
            </div>
          </div>
        </header>

      <div className="flex flex-wrap items-end gap-3 border-b border-subtle bg-raised/25 px-5 py-4 max-sm:flex-col max-sm:items-stretch">
        <div className="relative flex min-w-64 max-w-md flex-1 flex-col gap-1.5 max-sm:max-w-none" ref={taskPickerRef}>
          <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-tertiary">{strings.taskFilesAttachTo}</span>
          <button
            type="button"
            className={`w-full rounded-xl border bg-surface text-left text-sm text-primary shadow-sm transition-shadow ${taskMenuOpen ? "border-accent ring-2 ring-accent/15" : "border-default hover:border-accent"}`}
            onClick={() => setTaskMenuOpen((open) => !open)}
            aria-haspopup="listbox"
            aria-expanded={taskMenuOpen}
          >
            <span className="flex min-h-11 w-full items-center justify-between gap-3 px-3.5">
              <span className="min-w-0 flex-1 truncate font-medium">{tasks.find((task) => task.id === taskId)?.title ?? strings.taskFilesAttachTo}</span>
              <ChevronDown className={`shrink-0 transition-transform ${taskMenuOpen ? "rotate-180 text-accent" : ""}`} size={16} />
            </span>
          </button>
          {taskMenuOpen && (
            <div className="absolute inset-x-0 top-full z-dropdown mt-1 max-h-60 overflow-y-auto rounded-lg border border-default bg-surface p-1 shadow-lg" role="listbox">
              {tasks.map((task) => (
                <button
                  key={task.id}
                  type="button"
                  role="option"
                  aria-selected={task.id === taskId}
                  className={`w-full rounded-lg text-left text-sm hover:bg-accent-tint hover:text-accent ${task.id === taskId ? "bg-accent-tint font-semibold text-accent" : "text-primary"}`}
                  onClick={() => {
                    setTaskId(task.id);
                    setTaskMenuOpen(false);
                  }}
                >
                  <span className="flex min-h-10 items-center justify-between gap-2 px-3 py-2">
                    <span className="truncate">{task.title}</span>
                    {task.id === taskId && <Check className="shrink-0" size={16} />}
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>
        <input
          ref={fileRef}
          className="sr-only"
          type="file"
          multiple
          onChange={(event) => {
            if (event.target.files !== null) void upload(event.target.files);
          }}
        />
        <button
          type="button"
          className="rounded-xl bg-accent text-sm font-semibold text-on-accent shadow-sm transition-colors hover:bg-accent-hover disabled:cursor-wait disabled:opacity-60"
          onClick={() => fileRef.current?.click()}
          disabled={uploading}
        >
          <span className="flex min-h-11 items-center justify-center gap-2 px-4"><Upload size={17} />{uploading ? strings.taskUploading : strings.taskAddAttachment}</span>
        </button>
        <button type="button" className="rounded-xl border border-default bg-surface text-sm font-semibold text-primary shadow-sm transition-colors hover:bg-raised hover:text-accent" onClick={() => setDriveOpen(true)}>
          <span className="flex min-h-11 items-center justify-center gap-2 px-4"><HardDrive size={17} />{strings.taskChooseFromDrive}</span>
        </button>
      </div>
      {uploadError && <p className="m-0 border-b border-subtle bg-danger-tint px-5 py-3 text-sm font-medium text-danger">{strings.taskFilesUploadError}</p>}
      {files.length === 0 && (
        <div className={`m-5 flex min-h-72 flex-col items-center justify-center gap-3 rounded-2xl border border-dashed px-6 text-center transition-[border-color,background-color,transform] ${dragging ? "scale-[1.005] border-accent bg-accent-soft" : "border-default bg-raised/20"}`}>
          <span className={`grid size-14 place-items-center rounded-2xl text-accent transition-colors ${dragging ? "bg-surface shadow-sm" : "bg-accent-tint"}`}><Upload size={25} /></span>
          <div>
            <p className="m-0 text-sm font-semibold text-primary">{strings.taskFilesEmpty}</p>
            <p className="m-0 mt-1 text-sm text-tertiary">{strings.taskFilesDropHint}</p>
          </div>
        </div>
      )}
      {files.length > 0 && <div className="grid gap-3 p-5 md:grid-cols-2 xl:grid-cols-3">
      {files.map((f) => (
        <article key={f.id} className="group flex min-w-0 items-center gap-3 rounded-2xl border border-subtle bg-surface p-4 shadow-sm transition-[border-color,box-shadow] hover:border-default hover:shadow-md">
          <span className="grid size-11 shrink-0 place-items-center rounded-xl bg-accent-tint text-accent">
            <FileIcon filename={f.filename} />
          </span>
          <span className="flex min-w-0 flex-1 flex-col">
            <button type="button" className="w-full truncate text-left text-sm font-semibold text-primary hover:text-accent" onClick={() => onOpen(f.taskId)}>
              <span className="block truncate">{f.filename}</span>
            </button>
            <span className="truncate text-xs text-tertiary">
              {f.taskTitle} · {fileSize(f.size)}
            </span>
          </span>
          <button
            type="button"
            className="shrink-0 rounded-lg text-tertiary opacity-70 transition-colors hover:bg-raised hover:text-accent group-hover:opacity-100"
            onClick={() => void download(f)}
            aria-label={strings.taskDownload}
          >
            <span className="grid size-9 place-items-center"><Download size={16} /></span>
          </button>
        </article>
      ))}
      </div>}
      </section>
      {driveOpen && <DriveAttachmentPicker onAttach={attachDriveFiles} onClose={() => setDriveOpen(false)} />}
    </div>
  );
}
