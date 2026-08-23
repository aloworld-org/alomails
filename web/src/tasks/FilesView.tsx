// The Files tab: every attachment across a project's tasks, from the real
// /tasks/files roll-up. Click a name to open its task; download pulls the blob
// through the same authenticated blob endpoint mail attachments use.
import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Download, HardDrive, Paperclip, Upload } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type DriveNodeDto, type ProjectFileDto, type Task } from "../jmap";
import { Spinner } from "../ds";
import { EmptyState } from "../projects/parts";
import { DriveAttachmentPicker } from "./DriveAttachmentPicker";

const secondaryButton = "inline-flex min-h-10 items-center justify-center gap-2 rounded-lg bg-raised px-4 py-2 text-sm font-semibold text-primary transition-colors hover:bg-accent-tint hover:text-accent disabled:cursor-wait disabled:opacity-60";

function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
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
      className={`flex w-full flex-col px-6 py-4 ${files.length === 0 ? "min-h-[26rem]" : "max-w-3xl"}`}
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        void upload(event.dataTransfer.files);
      }}
    >
      <div className="mb-4 flex flex-wrap items-end gap-3 max-sm:flex-col max-sm:items-stretch">
        <div className="relative flex min-w-56 max-w-80 flex-1 flex-col gap-1.5 max-sm:max-w-none" ref={taskPickerRef}>
          <span className="text-xs font-semibold uppercase tracking-wide text-secondary">{strings.taskFilesAttachTo}</span>
          <button
            type="button"
            className={`flex min-h-10 w-full items-center justify-between gap-2 rounded-lg border bg-surface px-3 text-left text-sm text-primary transition-shadow ${taskMenuOpen ? "border-accent ring-2 ring-accent/15" : "border-default hover:border-accent"}`}
            onClick={() => setTaskMenuOpen((open) => !open)}
            aria-haspopup="listbox"
            aria-expanded={taskMenuOpen}
          >
            <span className="min-w-0 flex-1 truncate">{tasks.find((task) => task.id === taskId)?.title ?? strings.taskFilesAttachTo}</span>
            <ChevronDown className={`shrink-0 transition-transform ${taskMenuOpen ? "rotate-180 text-accent" : ""}`} size={16} />
          </button>
          {taskMenuOpen && (
            <div className="absolute inset-x-0 top-full z-dropdown mt-1 max-h-60 overflow-y-auto rounded-lg border border-default bg-surface p-1 shadow-lg" role="listbox">
              {tasks.map((task) => (
                <button
                  key={task.id}
                  type="button"
                  role="option"
                  aria-selected={task.id === taskId}
                  className={`flex min-h-10 w-full items-center justify-between gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-accent-tint hover:text-accent ${task.id === taskId ? "bg-accent-tint font-semibold text-accent" : "text-primary"}`}
                  onClick={() => {
                    setTaskId(task.id);
                    setTaskMenuOpen(false);
                  }}
                >
                  <span className="truncate">{task.title}</span>
                  {task.id === taskId && <Check className="shrink-0" size={16} />}
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
          className="inline-flex min-h-10 items-center justify-center gap-2 rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-on-accent hover:bg-accent-hover disabled:cursor-wait disabled:opacity-60"
          onClick={() => fileRef.current?.click()}
          disabled={uploading}
        >
          <Upload size={17} />
          {uploading ? strings.taskUploading : strings.taskAddAttachment}
        </button>
        <button type="button" className={secondaryButton} onClick={() => setDriveOpen(true)}>
          <HardDrive size={17} /> {strings.taskChooseFromDrive}
        </button>
      </div>
      {uploadError && <p className="mb-3 text-sm text-danger">{strings.taskFilesUploadError}</p>}
      {files.length === 0 && (
        <div className="flex min-h-80 flex-1 flex-col items-center justify-center gap-3 rounded-2xl border border-dashed border-default px-6 text-center">
          <span className="grid size-16 place-items-center rounded-2xl bg-accent-tint text-accent"><Paperclip size={32} /></span>
          <p className="m-0 max-w-sm text-sm leading-6 text-secondary">{strings.taskFilesEmpty}</p>
          <p className="m-0 text-sm text-tertiary">{strings.taskFilesDropHint}</p>
        </div>
      )}
      {files.map((f) => (
        <div key={f.id} className="mb-2 flex items-center gap-3 rounded-xl border border-subtle bg-surface p-3 transition-colors hover:border-default">
          <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-accent-tint text-accent">
            <Paperclip size={16} />
          </span>
          <span className="flex min-w-0 flex-1 flex-col">
            <button type="button" className="truncate text-left text-sm font-medium text-primary hover:text-accent" onClick={() => onOpen(f.taskId)}>
              {f.filename}
            </button>
            <span className="truncate text-xs text-tertiary">
              {f.taskTitle} · {fileSize(f.size)}
            </span>
          </span>
          <button
            type="button"
            className="grid size-9 shrink-0 place-items-center rounded-lg text-tertiary hover:bg-raised hover:text-accent"
            onClick={() => void download(f)}
            aria-label={strings.taskDownload}
          >
            <Download size={16} />
          </button>
        </div>
      ))}
      {driveOpen && <DriveAttachmentPicker onAttach={attachDriveFiles} onClose={() => setDriveOpen(false)} />}
    </div>
  );
}
