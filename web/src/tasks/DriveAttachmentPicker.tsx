// The Drive picker a task attaches files from: a folder browser with crumbs,
// a name filter, and a multi-select over the files that have content.
//
// The dialog is `ds/Modal` (D2.11), `tall` because it is a browser — its
// content changes with every keystroke, and a fixed-height panel is what keeps
// it from resizing under the pointer. The hand-rolled overlay this replaces
// wrote `z-modal` and `bg-scrim`, two classes the theme does not generate, so
// it shipped with no z-index and no scrim tint; and it opens over the create
// form, which is the case `Modal`'s stack exists for — Escape closes the
// picker and leaves the form standing.
import { useEffect, useMemo, useState } from "react";
import { Check, ChevronLeft, FileText, Folder, HardDrive, X } from "lucide-react";

import { strings } from "../i18n";
import { Button, IconButton, Input, Modal, Spinner } from "../ds";
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
    <Modal
      title={strings.taskChooseFromDrive}
      onClose={onClose}
      icon={<HardDrive size={19} />}
      wide
      tall
      actions={<IconButton label={strings.close} icon={<X size={18} />} onClick={onClose} />}
      footer={
        <>
          <span className="mr-auto text-sm text-secondary">{strings.taskFilesSelected(selectedNodes.length)}</span>
          <Button variant="ghost" onClick={onClose}>
            {strings.taskCancel}
          </Button>
          <Button
            disabled={selectedNodes.length === 0 || attaching}
            onClick={() => {
              setAttaching(true);
              void onAttach(selectedNodes).finally(() => setAttaching(false));
            }}
          >
            {attaching ? strings.taskUploading : strings.taskAttachSelected}
          </Button>
        </>
      }
    >
      <p className="m-0 text-sm text-tertiary">{strings.taskChooseFromDriveHint}</p>
      <div className="flex items-center gap-2 max-sm:flex-wrap">
        <IconButton
          label={strings.taskDriveBack}
          icon={<ChevronLeft size={18} />}
          disabled={crumbs.length === 1}
          onClick={() => setCrumbs((currentCrumbs) => currentCrumbs.slice(0, -1))}
        />
        <div className="min-w-20 flex-1 truncate text-sm text-secondary">{crumbs.map((crumb) => crumb.name).join(" / ")}</div>
        {/* `Input` is `w-full` by design; the span owns the column width. */}
        <span className="w-60 max-sm:order-last max-sm:w-full">
          <Input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={strings.taskSearchDrive}
            aria-label={strings.taskSearchDrive}
          />
        </span>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
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
              className={`flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left text-sm transition-colors hover:bg-accent-soft hover:text-accent ${chosen ? "bg-accent-soft text-accent" : "text-primary"}`}
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
    </Modal>
  );
}
