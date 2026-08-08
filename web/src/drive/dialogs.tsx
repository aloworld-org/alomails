// Drive modals: choose a destination (move/copy), browse version history, and
// view/manage a Space's membership. Each is a small focused dialog over the
// file manager.
import { useEffect, useState } from "react";
import { HardDrive, Users, X } from "lucide-react";

import { strings } from "../i18n";
import {
  useJmapClient,
  type DriveVersionDto,
  type SpaceDto,
  type SpaceDetailDto,
  type SpaceRole,
} from "../jmap";
import { Avatar, useDialogs } from "../ds";
import { driveErrorReason, fileSize } from "./parts";
import styles from "./DriveModule.module.css";

/** Pick a destination location (My Files or a writable Space) for move/copy. */
export function DestinationDialog({
  spaces,
  mode,
  onPick,
  onClose,
}: {
  spaces: SpaceDto[];
  mode: "move" | "copy";
  onPick: (space: string | null) => void;
  onClose: () => void;
}) {
  const writable = spaces.filter((s) => s.myRole !== "viewer" && !s.archived);
  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.dialog} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.dialogHead}>
          <h2>{mode === "move" ? strings.driveMoveTo : strings.driveCopyTo}</h2>
          <button type="button" className={styles.iconBtn} onClick={onClose} aria-label={strings.close}>
            <X size={18} />
          </button>
        </div>
        <div className={styles.destList}>
          <button type="button" className={styles.destItem} onClick={() => onPick(null)}>
            <HardDrive size={18} />
            <span>{strings.driveMyFiles}</span>
          </button>
          {writable.map((s) => (
            <button key={s.id} type="button" className={styles.destItem} onClick={() => onPick(s.id)}>
              <Users size={18} />
              <span>{s.name}</span>
            </button>
          ))}
        </div>
        <p className={styles.destHint}>{strings.driveDestHint}</p>
      </div>
    </div>
  );
}

/** A file's version history, with restore. */
export function VersionsDialog({ nodeId, onChanged, onClose }: { nodeId: string; onChanged: () => void; onClose: () => void }) {
  const client = useJmapClient();
  const [versions, setVersions] = useState<DriveVersionDto[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState("");

  const load = () => {
    let live = true;
    setVersions(null);
    setLoadError(null);
    void client
      .driveVersions(nodeId)
      .then((v) => live && setVersions(v))
      .catch((error: unknown) => {
        if (!live) return;
        setVersions([]);
        setLoadError(driveErrorReason(error) ?? strings.driveUnknownError);
      });
    return () => {
      live = false;
    };
  };

  useEffect(() => {
    const cancel = load();
    return () => {
      cancel();
    };
  }, [client, nodeId]);

  async function restore(no: number) {
    setActionError("");
    try {
      await client.driveRestoreVersion(nodeId, no);
      setVersions(await client.driveVersions(nodeId));
      onChanged();
    } catch (error) {
      setActionError(strings.driveActionFailed(strings.driveRestore, driveErrorReason(error) ?? strings.driveUnknownError));
    }
  }

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.dialog} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.dialogHead}>
          <h2>{strings.driveVersionHistory}</h2>
          <button type="button" className={styles.iconBtn} onClick={onClose} aria-label={strings.close}>
            <X size={18} />
          </button>
        </div>
        {versions === null ? (
          <DialogSkeleton />
        ) : loadError !== null ? (
          <div className={styles.dialogError} role="alert">
            <p>{strings.driveVersionsLoadFailed(loadError)}</p>
            <button type="button" className={styles.versionRestore} onClick={load}>{strings.driveRetry}</button>
          </div>
        ) : versions.length === 0 ? (
          <p className={styles.destHint}>{strings.driveNoVersions}</p>
        ) : (
          <ul className={styles.versionList}>
            {versions.map((v, i) => (
              <li key={v.versionNo} className={styles.versionRow}>
                <span className={styles.versionNo}>v{v.versionNo}</span>
                <span className={styles.versionMeta}>
                  {fileSize(v.size)} · {new Date(v.createdAt).toLocaleString()}
                </span>
                {i === 0 ? (
                  <span className={styles.versionCurrent}>{strings.driveCurrent}</span>
                ) : (
                  <button type="button" className={styles.versionRestore} onClick={() => void restore(v.versionNo)}>
                    {strings.driveRestore}
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
        {actionError !== "" && <p className={styles.memberErr} role="alert">{actionError}</p>}
      </div>
    </div>
  );
}

/** A Space's membership — who has access — with manage controls for managers. */
export function MembersDialog({ space, onClose }: { space: SpaceDto; onClose: () => void }) {
  const client = useJmapClient();
  const { confirm } = useDialogs();
  const [detail, setDetail] = useState<SpaceDetailDto | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<SpaceRole>("editor");
  const [error, setError] = useState("");

  const load = () => {
    setDetail(null);
    setLoadError(null);
    void client.spaceDetail(space.id).then(setDetail).catch((caught: unknown) => {
      setLoadError(driveErrorReason(caught) ?? strings.driveUnknownError);
    });
  };
  useEffect(load, [client, space.id]);

  const canManage = space.myRole === "manager";

  async function add() {
    const addr = email.trim();
    if (addr === "") return;
    setError("");
    try {
      await client.addSpaceMember(space.id, addr, role);
      setEmail("");
      load();
    } catch (caught) {
      setError(strings.driveActionFailed(strings.driveAdd, driveErrorReason(caught) ?? strings.driveUnknownError));
    }
  }

  async function remove(userId: string, who: string) {
    if (!(await confirm({ message: strings.driveRemoveMemberConfirm(who), danger: true }))) return;
    try {
      await client.removeSpaceMember(space.id, userId);
      load();
    } catch (caught) {
      setError(strings.driveActionFailed(strings.driveRemoveMember, driveErrorReason(caught) ?? strings.driveUnknownError));
    }
  }

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.dialog} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.dialogHead}>
          <h2>{strings.driveMembersOf(space.name)}</h2>
          <button type="button" className={styles.iconBtn} onClick={onClose} aria-label={strings.close}>
            <X size={18} />
          </button>
        </div>
        {detail === null && loadError === null ? (
          <DialogSkeleton />
        ) : loadError !== null ? (
          <div className={styles.dialogError} role="alert">
            <p>{strings.driveMembersLoadFailed(loadError)}</p>
            <button type="button" className={styles.versionRestore} onClick={load}>{strings.driveRetry}</button>
          </div>
        ) : detail !== null ? (
          <>
            <ul className={styles.memberList}>
              {detail.members.map((m) => (
                <li key={m.userId} className={styles.memberRow}>
                  <Avatar name={m.email ?? m.userId} />
                  <span className={styles.memberEmail}>{m.email ?? m.userId}</span>
                  <span className={styles.memberRole}>{strings.driveRole(m.role)}</span>
                  {canManage && (
                    <button
                      type="button"
                      className={styles.iconBtn}
                      onClick={() => void remove(m.userId, m.email ?? m.userId)}
                      aria-label={strings.driveRemoveMember}
                    >
                      <X size={14} />
                    </button>
                  )}
                </li>
              ))}
            </ul>
            {canManage && (
              <div className={styles.addMember}>
                <input
                  className={styles.addEmail}
                  value={email}
                  placeholder={strings.driveAddMemberPlaceholder}
                  inputMode="email"
                  autoCapitalize="none"
                  onChange={(e) => setEmail(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && void add()}
                />
                <select
                  className={styles.addRole}
                  value={role}
                  onChange={(e) => setRole(e.target.value as SpaceRole)}
                >
                  <option value="viewer">{strings.driveRole("viewer")}</option>
                  <option value="editor">{strings.driveRole("editor")}</option>
                  <option value="manager">{strings.driveRole("manager")}</option>
                </select>
                <button type="button" className={styles.addBtn} onClick={() => void add()}>
                  {strings.driveAdd}
                </button>
              </div>
            )}
            {error !== "" && <p className={styles.memberErr}>{error}</p>}
          </>
        ) : null}
      </div>
    </div>
  );
}

function DialogSkeleton() {
  return (
    <div className={styles.dialogSkeleton} role="status" aria-label={strings.driveLoading} aria-busy="true">
      {Array.from({ length: 4 }, (_, index) => <span key={index} />)}
    </div>
  );
}
