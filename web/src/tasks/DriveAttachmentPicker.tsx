import { useEffect, useMemo, useState } from "react";
import { Check, ChevronLeft, FileText, Folder, HardDrive, Search, X } from "lucide-react";

import { strings } from "../i18n";
import { Spinner } from "../ds";
import { useJmapClient, type DriveNodeDto } from "../jmap";
import styles from "./TasksModule.module.css";

interface Props {
  onAttach: (nodes: DriveNodeDto[]) => Promise<void>;
  onClose: () => void;
}

interface FolderCrumb {
  id: string | null;
  name: string;
}

export function DriveAttachmentPicker({ onAttach, onClose }: Props) {
  const client = useJmapClient();
  const [crumbs, setCrumbs] = useState<FolderCrumb[]>([{ id: null, name: strings.driveMyFiles }]);
  const [nodes, setNodes] = useState<DriveNodeDto[] | null>(null);
  const [selected, setSelected] = useState<ReadonlyMap<string, DriveNodeDto>>(new Map());
  const [query, setQuery] = useState("");
  const [attaching, setAttaching] = useState(false);
  const current = crumbs[crumbs.length - 1];

  useEffect(() => {
    let live = true;
    setNodes(null);
    void client.driveList(null, current?.id ?? null).then((next) => {
      if (live) setNodes(next);
    }).catch(() => {
      if (live) setNodes([]);
    });
    return () => { live = false; };
  }, [client, current?.id]);

  useEffect(() => {
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose]);

  const visible = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    if (nodes === null || term === "") return nodes;
    return nodes.filter((node) => node.name.toLocaleLowerCase().includes(term));
  }, [nodes, query]);

  const selectedNodes = Array.from(selected.values());

  function toggle(node: DriveNodeDto) {
    setSelected((currentSelected) => {
      const next = new Map(currentSelected);
      if (next.has(node.id)) next.delete(node.id);
      else next.set(node.id, node);
      return next;
    });
  }

  return (
    <div className={styles.drivePickerBackdrop} onMouseDown={onClose}>
      <section
        className={styles.drivePicker}
        role="dialog"
        aria-modal="true"
        aria-labelledby="drive-picker-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className={styles.drivePickerHead}>
          <span className={styles.drivePickerMark}><HardDrive size={20} /></span>
          <div>
            <h2 id="drive-picker-title">{strings.taskChooseFromDrive}</h2>
            <p>{strings.taskChooseFromDriveHint}</p>
          </div>
          <button type="button" onClick={onClose} aria-label={strings.close}><X size={19} /></button>
        </header>

        <div className={styles.drivePickerTools}>
          <button
            type="button"
            className={styles.drivePickerBack}
            disabled={crumbs.length === 1}
            onClick={() => setCrumbs((currentCrumbs) => currentCrumbs.slice(0, -1))}
            aria-label={strings.taskDriveBack}
          >
            <ChevronLeft size={18} />
          </button>
          <div className={styles.drivePickerPath}>{crumbs.map((crumb) => crumb.name).join(" / ")}</div>
          <label className={styles.drivePickerSearch}>
            <Search size={16} />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={strings.taskSearchDrive}
            />
          </label>
        </div>

        <div className={styles.drivePickerList}>
          {visible === null ? (
            <div className={styles.drivePickerStatus}><Spinner size={22} /></div>
          ) : visible.length === 0 ? (
            <div className={styles.drivePickerStatus}>{strings.taskNoDriveFiles}</div>
          ) : visible.map((node) => {
            const folder = node.kind === "folder";
            const chosen = selected.has(node.id);
            return (
              <button
                key={node.id}
                type="button"
                className={`${styles.drivePickerRow} ${chosen ? styles.drivePickerRowSelected : ""}`}
                onClick={() => {
                  if (folder) {
                    setCrumbs((currentCrumbs) => [...currentCrumbs, { id: node.id, name: node.name }]);
                    setQuery("");
                  } else if (node.blobId !== null) toggle(node);
                }}
              >
                <span className={folder ? styles.driveFolderIcon : styles.driveFileIcon}>
                  {folder ? <Folder size={20} /> : <FileText size={20} />}
                </span>
                <span className={styles.drivePickerName}>{node.name}</span>
                {!folder && node.blobId !== null && (
                  <span className={styles.drivePickerCheck}>{chosen && <Check size={16} />}</span>
                )}
              </button>
            );
          })}
        </div>

        <footer className={styles.drivePickerFoot}>
          <span>{strings.taskFilesSelected(selectedNodes.length)}</span>
          <button type="button" className={styles.drivePickerCancel} onClick={onClose}>{strings.taskCancel}</button>
          <button
            type="button"
            className={styles.filesUpload}
            disabled={selectedNodes.length === 0 || attaching}
            onClick={() => {
              setAttaching(true);
              void onAttach(selectedNodes).finally(() => setAttaching(false));
            }}
          >
            {attaching ? strings.taskUploading : strings.taskAttachSelected}
          </button>
        </footer>
      </section>
    </div>
  );
}
