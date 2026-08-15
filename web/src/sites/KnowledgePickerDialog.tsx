// Publishing one Drive document to the site's assistant (ADR 0040 §1,
// item S3.02d). One document at a time, chosen by browsing — deliberately no
// way to select a folder, because "this folder and whatever lands in it" is
// exactly the standing grant the ADR rejects. The server rules on what is
// readable (an alo Doc, a PDF, an Office file, plain text); its refusal is
// shown verbatim.
//
// Above the confirm button, every time, the sentence that IS the permission
// model: **anyone on the internet will be able to read this.**
import { useEffect, useMemo, useState } from "react";
import { BookOpenCheck, ChevronLeft, FileText, Folder, Search } from "lucide-react";

import { strings } from "../i18n";
import { Spinner } from "../ds";
import { useJmapClient, type DriveNodeDto } from "../jmap";
import { sitesMessage, useSitesApi } from "./api";
import { DialogFrame } from "./parts";
import type { SiteKnowledgeSource } from "./types";
import styles from "./SitesModule.module.css";

interface Crumb {
  id: string | null;
  name: string;
}

export function KnowledgePickerDialog({
  siteId,
  onClose,
  onPublished,
}: {
  siteId: string;
  onClose: () => void;
  onPublished: (source: SiteKnowledgeSource) => void;
}) {
  const client = useJmapClient();
  const api = useSitesApi();
  const [crumbs, setCrumbs] = useState<Crumb[]>([
    { id: null, name: strings.driveMyFiles },
  ]);
  const [nodes, setNodes] = useState<DriveNodeDto[] | null>(null);
  const [selected, setSelected] = useState<DriveNodeDto | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const folder = crumbs[crumbs.length - 1];

  useEffect(() => {
    let live = true;
    setNodes(null);
    client
      .driveList(null, folder?.id ?? null)
      .then((next) => {
        if (live) setNodes(next.filter((node) => !node.trashed));
      })
      .catch(() => {
        if (live) setNodes([]);
      });
    return () => {
      live = false;
    };
  }, [client, folder?.id]);

  const visible = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    if (nodes === null || term === "") return nodes;
    return nodes.filter((node) => node.name.toLocaleLowerCase().includes(term));
  }, [nodes, query]);

  function publish() {
    if (selected === null) return;
    setBusy(true);
    setError(null);
    api.addChatKnowledge(siteId, selected.id).then(
      (source) => {
        setBusy(false);
        onPublished(source);
      },
      (err: unknown) => {
        setBusy(false);
        setError(sitesMessage(err, strings.sitesAssistantPublishFailed));
      },
    );
  }

  return (
    <DialogFrame
      Icon={BookOpenCheck}
      title={strings.sitesAssistantPickerTitle}
      subtitle={strings.sitesAssistantPickerSubtitle}
      error={error}
      busy={busy}
      canSubmit={selected !== null}
      submitLabel={strings.sitesAssistantPickerConfirm}
      wide
      onClose={onClose}
      onSubmit={publish}
    >
      <div className={styles.assistantPickerTools}>
        <button
          type="button"
          className={styles.assistantPickerBack}
          disabled={crumbs.length === 1}
          onClick={() => setCrumbs((current) => current.slice(0, -1))}
          aria-label={strings.sitesAssistantPickerBack}
        >
          <ChevronLeft size={18} aria-hidden="true" />
        </button>
        <span className={styles.assistantPickerPath}>
          {crumbs.map((crumb) => crumb.name).join(" / ")}
        </span>
        <label className={styles.assistantPickerSearch}>
          <Search size={16} aria-hidden="true" />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={strings.sitesAssistantPickerSearch}
            aria-label={strings.sitesAssistantPickerSearch}
          />
        </label>
      </div>

      <div className={styles.assistantPickerList}>
        {visible === null ? (
          <div className={styles.assistantPickerStatus}>
            <Spinner size={20} />
          </div>
        ) : visible.length === 0 ? (
          <div className={styles.assistantPickerStatus}>
            {strings.sitesAssistantPickerEmpty}
          </div>
        ) : (
          visible.map((node) => {
            const isFolder = node.kind === "folder";
            const chosen = selected?.id === node.id;
            return (
              <button
                key={node.id}
                type="button"
                className={
                  chosen
                    ? `${styles.assistantPickerRow} ${styles.assistantPickerRowSelected}`
                    : styles.assistantPickerRow
                }
                aria-pressed={isFolder ? undefined : chosen}
                onClick={() => {
                  if (isFolder) {
                    setCrumbs((current) => [...current, { id: node.id, name: node.name }]);
                    setQuery("");
                    setSelected(null);
                  } else {
                    setSelected((current) => (current?.id === node.id ? null : node));
                  }
                }}
              >
                {isFolder ? (
                  <Folder size={18} aria-hidden="true" />
                ) : (
                  <FileText size={18} aria-hidden="true" />
                )}
                <span className={styles.assistantPickerName}>{node.name}</span>
              </button>
            );
          })
        )}
      </div>

      {/* Above the button, every time — the sentence is the boundary. */}
      <p className={styles.assistantWarning}>
        {strings.sitesAssistantInternetWarning}
      </p>
    </DialogFrame>
  );
}
