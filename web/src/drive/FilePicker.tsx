// Choose a file from Drive.
//
// The first picker in the app, so it is built here in `drive/` as a component
// any module can mount — chat is simply the first caller. It reads Drive with
// the same client the Drive module uses; there is no second idea of what a
// folder is.
//
// It says who will be able to open what is chosen. A file in someone's
// personal Drive is readable by them alone, so sharing one into a room shows
// everybody else a message with an attachment they cannot open. The picker
// does not forbid that — sending a colleague a file in a DM they already have
// access to is legitimate, and so is attaching your own note to your own
// message — but it never lets it happen silently (UX law 6: no surprise).
import { useCallback, useEffect, useState } from "react";
import { ChevronRight, File, Folder, Loader2, Lock, Users } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import type { DriveNodeDto, SpaceDto } from "../jmap/types";
import { fileSize } from "./parts";
import styles from "./FilePicker.module.css";

/** Where the picker is currently looking. */
interface Place {
  /** `null` is the caller's personal Drive. */
  space: string | null;
  /** Folder within that place; `null` is its root. */
  parent: string | null;
  /** The path back out, innermost last. */
  trail: { id: string | null; name: string }[];
}

export function FilePicker({
  onPick,
  onClose,
  max = 10,
}: {
  /** Called with the chosen files. Never called with more than `max`. */
  onPick: (files: DriveNodeDto[]) => void;
  onClose: () => void;
  max?: number;
}) {
  const client = useJmapClient();
  const [spaces, setSpaces] = useState<SpaceDto[] | null>(null);
  const [place, setPlace] = useState<Place>({
    space: null,
    parent: null,
    trail: [],
  });
  const [nodes, setNodes] = useState<DriveNodeDto[] | null>(null);
  const [chosen, setChosen] = useState<DriveNodeDto[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void client
      .spaces()
      .then((all) => setSpaces(all.filter((s) => !s.archived)))
      .catch(() => setSpaces([]));
  }, [client]);

  const look = useCallback(async () => {
    setNodes(null);
    try {
      setNodes(await client.driveList(place.space, place.parent));
    } catch {
      setError(strings.pickerLoadFailed);
      setNodes([]);
    }
  }, [client, place.space, place.parent]);

  useEffect(() => {
    void look();
  }, [look]);

  function openFolder(node: DriveNodeDto) {
    setPlace((at) => ({
      space: at.space,
      parent: node.id,
      trail: [...at.trail, { id: node.id, name: node.name }],
    }));
  }

  function goTo(index: number) {
    setPlace((at) => {
      const trail = at.trail.slice(0, index);
      return {
        space: at.space,
        parent: trail[trail.length - 1]?.id ?? null,
        trail,
      };
    });
  }

  function switchPlace(space: string | null) {
    setChosen([]);
    setPlace({ space, parent: null, trail: [] });
  }

  function toggle(node: DriveNodeDto) {
    setChosen((held) => {
      const already = held.some((f) => f.id === node.id);
      if (already) return held.filter((f) => f.id !== node.id);
      if (held.length >= max) return held; // the ceiling the server enforces
      return [...held, node];
    });
  }

  const folders = (nodes ?? []).filter((n) => n.kind === "folder");
  const files = (nodes ?? []).filter((n) => n.kind !== "folder");
  const here = spaces?.find((s) => s.id === place.space) ?? null;

  return (
    <div
      className={styles.backdrop}
      role="dialog"
      aria-modal="true"
      aria-label={strings.pickerTitle}
    >
      <div className={styles.panel}>
        <header className={styles.header}>
          <h2 className={styles.title}>{strings.pickerTitle}</h2>
        </header>

        <div className={styles.body}>
          <nav className={styles.places} aria-label={strings.pickerPlaces}>
            <button
              type="button"
              className={place.space === null ? styles.placeOpen : styles.place}
              onClick={() => switchPlace(null)}
            >
              <Lock size={14} /> {strings.pickerMyDrive}
            </button>
            {(spaces ?? []).map((space) => (
              <button
                key={space.id}
                type="button"
                className={
                  place.space === space.id ? styles.placeOpen : styles.place
                }
                onClick={() => switchPlace(space.id)}
              >
                <Users size={14} /> {space.name}
              </button>
            ))}
          </nav>

          <div className={styles.listing}>
            <div className={styles.trail}>
              <button
                type="button"
                className={styles.crumb}
                onClick={() => goTo(0)}
              >
                {here?.name ?? strings.pickerMyDrive}
              </button>
              {place.trail.map((step, i) => (
                <span key={step.id ?? i} className={styles.crumbWrap}>
                  <ChevronRight size={13} className={styles.crumbSep} />
                  <button
                    type="button"
                    className={styles.crumb}
                    onClick={() => goTo(i + 1)}
                  >
                    {step.name}
                  </button>
                </span>
              ))}
            </div>

            {/* Law 6: say what choosing here will mean, before it is chosen. */}
            {place.space === null && (
              <p className={styles.notice}>{strings.pickerPersonalNotice}</p>
            )}

            {nodes === null ? (
              <p className={styles.note}>
                <Loader2 className={styles.spin} size={14} />{" "}
                {strings.pickerLoading}
              </p>
            ) : folders.length + files.length === 0 ? (
              <p className={styles.note}>{strings.pickerEmpty}</p>
            ) : (
              <ul className={styles.items}>
                {folders.map((node) => (
                  <li key={node.id}>
                    <button
                      type="button"
                      className={styles.row}
                      onClick={() => openFolder(node)}
                    >
                      <Folder size={16} className={styles.icon} />
                      <span className={styles.name}>{node.name}</span>
                      <ChevronRight size={14} className={styles.icon} />
                    </button>
                  </li>
                ))}
                {files.map((node) => {
                  const picked = chosen.some((f) => f.id === node.id);
                  return (
                    <li key={node.id}>
                      <button
                        type="button"
                        className={picked ? styles.rowPicked : styles.row}
                        onClick={() => toggle(node)}
                        aria-pressed={picked}
                      >
                        <File size={16} className={styles.icon} />
                        <span className={styles.name}>{node.name}</span>
                        <span className={styles.size}>
                          {fileSize(node.size)}
                        </span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
            {error !== null && <p className={styles.error}>{error}</p>}
          </div>
        </div>

        <footer className={styles.footer}>
          <span className={styles.count}>
            {chosen.length === 0
              ? strings.pickerNonePicked
              : strings.pickerPicked(chosen.length, max)}
          </span>
          <span className={styles.actions}>
            <Button variant="ghost" onClick={onClose}>
              {strings.cancel}
            </Button>
            <Button
              variant="primary"
              disabled={chosen.length === 0}
              onClick={() => onPick(chosen)}
            >
              {strings.pickerAttach}
            </Button>
          </span>
        </footer>
      </div>
    </div>
  );
}
