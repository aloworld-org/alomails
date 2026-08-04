// The Files tab: every attachment across a project's tasks, from the real
// /tasks/files roll-up. Click a name to open its task; download pulls the blob
// through the same authenticated blob endpoint mail attachments use.
import { useEffect, useState } from "react";
import { Download, Paperclip } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type ProjectFileDto } from "../jmap";
import { Spinner } from "../ds";
import styles from "./TasksModule.module.css";

function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

interface Props {
  projectId: string;
  onOpen: (taskId: string) => void;
}

export function FilesView({ projectId, onOpen }: Props) {
  const client = useJmapClient();
  const [files, setFiles] = useState<ProjectFileDto[] | null>(null);

  useEffect(() => {
    let live = true;
    client
      .projectFiles(projectId)
      .then((f) => {
        if (live) setFiles(f);
      })
      .catch(() => {
        if (live) setFiles([]);
      });
    return () => {
      live = false;
    };
  }, [client, projectId]);

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

  if (files === null) {
    return (
      <div className={styles.empty}>
        <Spinner size={20} />
      </div>
    );
  }
  if (files.length === 0) {
    return (
      <div className={styles.emptyState}>
        <span className={styles.emptyArt}>
          <Paperclip size={36} />
        </span>
        <p className={styles.emptyBody}>{strings.taskFilesEmpty}</p>
      </div>
    );
  }

  return (
    <div className={styles.filesView}>
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
    </div>
  );
}
