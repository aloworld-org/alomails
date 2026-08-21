import { useEffect, useMemo, useState } from "react";
import { Check, ChevronLeft, FileText, Folder, HardDrive, Search, X } from "lucide-react";

import { strings } from "../i18n";
import { Spinner } from "../ds";
import { useJmapClient, type DriveNodeDto } from "../jmap";

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
    <div className="fixed inset-0 z-modal grid place-items-center bg-scrim/55 p-5 backdrop-blur-sm" onMouseDown={onClose}>
      <section
        className="flex h-[min(40rem,calc(100vh-2.5rem))] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-default bg-surface shadow-xl"
        role="dialog"
        aria-modal="true"
        aria-labelledby="drive-picker-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="flex items-center gap-3 border-b border-subtle p-5">
          <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-tint text-accent"><HardDrive size={20} /></span>
          <div className="min-w-0 flex-1">
            <h2 id="drive-picker-title" className="m-0 text-lg font-semibold text-primary">{strings.taskChooseFromDrive}</h2>
            <p className="mt-1 text-sm text-secondary">{strings.taskChooseFromDriveHint}</p>
          </div>
          <button type="button" className="grid size-9 place-items-center rounded-lg text-secondary hover:bg-accent-tint hover:text-accent" onClick={onClose} aria-label={strings.close}><X size={19} /></button>
        </header>

        <div className="flex items-center gap-2 border-b border-subtle px-4 py-3 max-sm:flex-wrap">
          <button
            type="button"
            className="grid size-9 shrink-0 place-items-center rounded-lg text-secondary hover:bg-raised hover:text-accent disabled:opacity-35"
            disabled={crumbs.length === 1}
            onClick={() => setCrumbs((currentCrumbs) => currentCrumbs.slice(0, -1))}
            aria-label={strings.taskDriveBack}
          >
            <ChevronLeft size={18} />
          </button>
          <div className="min-w-20 flex-1 truncate text-sm text-secondary">{crumbs.map((crumb) => crumb.name).join(" / ")}</div>
          <label className="flex min-h-9 w-60 items-center gap-2 rounded-lg border border-default px-3 text-tertiary focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/15 max-sm:order-last max-sm:w-full">
            <Search size={16} />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="min-w-0 flex-1 border-0 bg-transparent text-sm text-primary outline-none placeholder:text-tertiary"
              placeholder={strings.taskSearchDrive}
            />
          </label>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          {visible === null ? (
            <div className="grid min-h-60 place-items-center text-sm text-tertiary"><Spinner size={22} /></div>
          ) : visible.length === 0 ? (
            <div className="grid min-h-60 place-items-center text-sm text-tertiary">{strings.taskNoDriveFiles}</div>
          ) : visible.map((node) => {
            const folder = node.kind === "folder";
            const chosen = selected.has(node.id);
            return (
              <button
                key={node.id}
                type="button"
                className={`flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left text-sm transition-colors ${chosen ? "bg-accent-tint text-accent" : "text-primary hover:bg-raised"}`}
                onClick={() => {
                  if (folder) {
                    setCrumbs((currentCrumbs) => [...currentCrumbs, { id: node.id, name: node.name }]);
                    setQuery("");
                  } else if (node.blobId !== null) toggle(node);
                }}
              >
                <span className={`grid size-9 shrink-0 place-items-center rounded-lg ${folder ? "bg-accent-tint text-accent" : "bg-raised text-secondary"}`}>
                  {folder ? <Folder size={20} /> : <FileText size={20} />}
                </span>
                <span className="min-w-0 flex-1 truncate">{node.name}</span>
                {!folder && node.blobId !== null && (
                  <span className={`grid size-6 shrink-0 place-items-center rounded-md border ${chosen ? "border-accent bg-accent text-on-accent" : "border-default"}`}>{chosen && <Check size={16} />}</span>
                )}
              </button>
            );
          })}
        </div>

        <footer className="flex flex-wrap items-center gap-3 border-t border-subtle p-4 text-sm text-secondary">
          <span className="mr-auto">{strings.taskFilesSelected(selectedNodes.length)}</span>
          <button type="button" className="min-h-10 rounded-lg bg-raised px-4 py-2 font-semibold text-primary hover:bg-accent-tint" onClick={onClose}>{strings.taskCancel}</button>
          <button
            type="button"
            className="min-h-10 rounded-lg bg-accent px-5 py-2 font-semibold text-on-accent hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
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
