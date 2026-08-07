// The Files tab: every attachment across a project's tasks, from the real
// /tasks/files roll-up. Click a name to open its task; download pulls the blob
// through the same authenticated blob endpoint mail attachments use.
import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Download, HardDrive, Paperclip, Upload } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type DriveNodeDto, type ProjectFileDto, type Task } from "../jmap";
import { Spinner } from "../ds";
import { DriveAttachmentPicker } from "./DriveAttachmentPicker";
import styles from "./TasksModule.module.css";

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
      <div className={styles.empty}>
        <Spinner size={20} />
      </div>
    );
  }
  if (tasks.length === 0) {
    return (
      <div className={styles.emptyState}>
        <span className={styles.emptyArt}>
          <Paperclip size={36} />
        </span>
        <p className={styles.emptyBody}>{strings.taskFilesNeedTask}</p>
        <button type="button" className={styles.emptyCta} onClick={onCreate}>
          {strings.taskCreateFirst}
        </button>
      </div>
    );
  }

  return (
    <div
      className={`${styles.filesView} ${files.length === 0 ? styles.filesViewEmpty : ""}`}
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        void upload(event.dataTransfer.files);
      }}
    >
      <div className={styles.filesToolbar}>
        <div className={styles.filesTaskPicker} ref={taskPickerRef}>
          <span>{strings.taskFilesAttachTo}</span>
          <button
            type="button"
            className={`${styles.filesTaskTrigger} ${taskMenuOpen ? styles.filesTaskTriggerOpen : ""}`}
            onClick={() => setTaskMenuOpen((open) => !open)}
            aria-haspopup="listbox"
            aria-expanded={taskMenuOpen}
          >
            <span>{tasks.find((task) => task.id === taskId)?.title ?? strings.taskFilesAttachTo}</span>
            <ChevronDown size={16} />
          </button>
          {taskMenuOpen && (
            <div className={styles.filesTaskMenu} role="listbox">
              {tasks.map((task) => (
                <button
                  key={task.id}
                  type="button"
                  role="option"
                  aria-selected={task.id === taskId}
                  className={task.id === taskId ? styles.filesTaskOptionSelected : ""}
                  onClick={() => {
                    setTaskId(task.id);
                    setTaskMenuOpen(false);
                  }}
                >
                  <span>{task.title}</span>
                  {task.id === taskId && <Check size={16} />}
                </button>
              ))}
            </div>
          )}
        </div>
        <input
          ref={fileRef}
          className={styles.visuallyHidden}
          type="file"
          multiple
          onChange={(event) => {
            if (event.target.files !== null) void upload(event.target.files);
          }}
        />
        <button
          type="button"
          className={styles.filesUpload}
          onClick={() => fileRef.current?.click()}
          disabled={uploading}
        >
          <Upload size={17} />
          {uploading ? strings.taskUploading : strings.taskAddAttachment}
        </button>
        <button type="button" className={styles.filesDriveButton} onClick={() => setDriveOpen(true)}>
          <HardDrive size={17} /> {strings.taskChooseFromDrive}
        </button>
      </div>
      {uploadError && <p className={styles.filesError}>{strings.taskFilesUploadError}</p>}
      {files.length === 0 && (
        <div className={styles.filesDropEmpty}>
          <span className={styles.emptyArt}><Paperclip size={36} /></span>
          <p className={styles.emptyBody}>{strings.taskFilesEmpty}</p>
          <p className={styles.filesDropHint}>{strings.taskFilesDropHint}</p>
        </div>
      )}
      {files.map((f) => (
        <div key={f.id} className={styles.fileRow}>
          <span className={styles.fileIcon}>
            <Paperclip size={16} />
          </span>
          <span className={styles.fileMeta}>
            <button type="button" className={styles.fileName} onClick={() => onOpen(f.taskId)}>
              {f.filename}
            </button>
            <span className={styles.fileSub}>
              {f.taskTitle} · {fileSize(f.size)}
            </span>
          </span>
          <button
            type="button"
            className={styles.fileDl}
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
