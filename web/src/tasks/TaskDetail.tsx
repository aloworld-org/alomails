// The task detail: a panel that slides in from the right and never navigates
// away (ADR 0021). Editable title/description, the status/priority/assignee/due
// fields, the source link (jump back to the email/event it came from), a
// subtask checklist, comments, and the activity history. Field edits persist on
// change; a change bubbles up so the board/list behind stays in sync.
import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  AlignLeft,
  CalendarDays,
  CheckCircle2,
  Circle,
  FolderClosed,
  Link2,
  Trash2,
  User,
  X,
} from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type TaskDetailData, type TaskInput, type TaskPriority } from "../jmap";
import { Spinner } from "../ds";
import { COLUMNS } from "./parts";
import styles from "./TasksModule.module.css";

interface Props {
  taskId: string;
  projectName?: string | undefined;
  onClose: () => void;
  /** Called after any change so the board/list can refresh. */
  onChanged: () => void;
}

export function TaskDetail({ taskId, projectName, onClose, onChanged }: Props) {
  const client = useJmapClient();
  const navigate = useNavigate();
  const [data, setData] = useState<TaskDetailData | null>(null);
  const [newSub, setNewSub] = useState("");
  const [newComment, setNewComment] = useState("");

  const load = useCallback(async () => {
    try {
      setData(await client.taskDetail(taskId));
    } catch {
      /* keep what we have */
    }
  }, [client, taskId]);

  useEffect(() => {
    void load();
  }, [load]);

  if (data === null) {
    return (
      <div className={styles.detailScrim} onMouseDown={onClose}>
        <div className={styles.detail} onMouseDown={(e) => e.stopPropagation()}>
          <div className={styles.detailBody} style={{ alignItems: "center" }}>
            <Spinner size={20} />
          </div>
        </div>
      </div>
    );
  }

  const t = data.task;

  /** Persist the editable fields (everything except status, which is a move).
   *  In `next`, a field is unchanged when omitted, cleared when `null`. */
  async function save(next: {
    title?: string;
    description?: string | null;
    assignee?: string | null;
    dueAt?: string | null;
    priority?: TaskPriority;
  }) {
    const cur = data!.task;
    const input: TaskInput = { title: next.title ?? cur.title, priority: next.priority ?? cur.priority };
    const description = next.description === undefined ? cur.description : next.description;
    if (description) input.description = description;
    const assignee = next.assignee === undefined ? cur.assignee : next.assignee;
    if (assignee) input.assignee = assignee;
    const dueAt = next.dueAt === undefined ? cur.dueAt : next.dueAt;
    if (dueAt) input.dueAt = dueAt;
    try {
      await client.updateTask(cur.id, input);
      await load();
      onChanged();
    } catch {
      /* ignore */
    }
  }

  async function changeStatus(status: string) {
    try {
      await client.moveTask(t.id, status, t.position);
      await load();
      onChanged();
    } catch {
      /* ignore */
    }
  }

  const dueDate = t.dueAt ? t.dueAt.slice(0, 10) : "";
  const done = t.status === "done";
  const subDone = data.subtasks.filter((s) => s.done).length;
  const subTotal = data.subtasks.length;
  const prioClass =
    t.priority === "high"
      ? styles.prioDotHigh
      : t.priority === "medium"
        ? styles.prioDotMedium
        : t.priority === "low"
          ? styles.prioDotLow
          : "";

  return (
    <div className={styles.detailScrim} onMouseDown={onClose}>
      <div className={styles.detail} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.tdHead}>
          <select
            className={styles.tdStatus}
            value={t.status}
            onChange={(e) => void changeStatus(e.target.value)}
          >
            {COLUMNS.map((c) => (
              <option key={c.key} value={c.key}>
                {c.label()}
              </option>
            ))}
          </select>
          <button
            type="button"
            className={styles.tdDelete}
            onClick={async () => {
              await client.deleteTask(t.id);
              onChanged();
              onClose();
            }}
            aria-label={strings.taskDelete}
          >
            <Trash2 size={16} />
          </button>
          <button type="button" className={styles.iconBtn} onClick={onClose} aria-label={strings.taskClose}>
            <X size={18} />
          </button>
        </div>

        <div className={styles.detailBody}>
          <div className={styles.tdTitleRow}>
            <button
              type="button"
              className={styles.tdTitleCheck}
              onClick={() => void changeStatus(done ? "todo" : "done")}
              aria-label={done ? strings.taskMarkNotDone : strings.taskMarkDone}
            >
              {done ? <CheckCircle2 size={22} /> : <Circle size={22} />}
            </button>
            <input
              className={`${styles.tdTitle} ${done ? styles.tdTitleDone : ""}`}
              defaultValue={t.title}
              onBlur={(e) => {
                if (e.target.value.trim() && e.target.value !== t.title) void save({ title: e.target.value.trim() });
              }}
            />
          </div>

          {t.sourceKind === "email" && t.sourceId && (
            <button
              type="button"
              className={styles.sourceLink}
              onClick={() => {
                onClose();
                navigate(`/mail?open=${encodeURIComponent(t.sourceId!)}`);
              }}
            >
              <Link2 size={14} /> {strings.taskOpenEmail}
            </button>
          )}
          {t.sourceKind === "event" && (
            <span className={styles.sourceLink}>
              <Link2 size={14} /> {strings.taskFromEvent}
            </span>
          )}

          <div className={styles.tdFields}>
            <label className={styles.tdField}>
              <span className={styles.tdFieldLabel}>
                <User size={15} /> {strings.taskAssignee}
              </span>
              <input
                className={styles.tdFieldInput}
                defaultValue={t.assignee ?? ""}
                placeholder={strings.taskAssigneePlaceholder}
                inputMode="email"
                autoCapitalize="none"
                onBlur={(e) => {
                  if ((e.target.value || "") !== (t.assignee ?? "")) void save({ assignee: e.target.value || null });
                }}
              />
            </label>
            {projectName !== undefined && projectName !== "" && (
              <div className={styles.tdField}>
                <span className={styles.tdFieldLabel}>
                  <FolderClosed size={15} /> {strings.taskColProject}
                </span>
                <span className={styles.tdFieldValue}>{projectName}</span>
              </div>
            )}
            <label className={styles.tdField}>
              <span className={styles.tdFieldLabel}>
                <CalendarDays size={15} /> {strings.taskDue}
              </span>
              <input
                className={styles.tdFieldInput}
                type="date"
                defaultValue={dueDate}
                onChange={(e) => void save({ dueAt: e.target.value ? `${e.target.value}T12:00:00Z` : null })}
              />
            </label>
            <label className={styles.tdField}>
              <span className={styles.tdFieldLabel}>
                <span className={`${styles.prioDot} ${prioClass}`} aria-hidden /> {strings.taskPriority}
              </span>
              <select
                className={styles.tdFieldInput}
                value={t.priority}
                onChange={(e) => void save({ priority: e.target.value as TaskPriority })}
              >
                <option value="none">{strings.taskPrioNone}</option>
                <option value="low">{strings.taskPrioLow}</option>
                <option value="medium">{strings.taskPrioMedium}</option>
                <option value="high">{strings.taskPrioHigh}</option>
              </select>
            </label>
          </div>

          <div className={styles.tdSection}>
            <span className={styles.tdSectionLabel}>
              <AlignLeft size={15} /> {strings.taskDescription}
            </span>
            <textarea
              className={styles.tdDescription}
              rows={3}
              defaultValue={t.description ?? ""}
              placeholder={strings.taskDescriptionPlaceholder}
              onBlur={(e) => {
                if ((e.target.value || "") !== (t.description ?? "")) void save({ description: e.target.value || null });
              }}
            />
          </div>

          <div className={styles.tdSection}>
            <div className={styles.tdSubHead}>
              <span className={styles.tdSectionLabel}>{strings.taskSubtasks}</span>
              {subTotal > 0 && (
                <span className={styles.tdProgressWrap}>
                  <span className={styles.tdProgressText}>
                    {subDone}/{subTotal}
                  </span>
                  <span className={styles.tdProgressBar}>
                    <span
                      className={styles.tdProgressFill}
                      style={{ width: `${subTotal === 0 ? 0 : (subDone / subTotal) * 100}%` }}
                    />
                  </span>
                </span>
              )}
            </div>
            <div className={styles.subtasks}>
              {data.subtasks.map((s) => (
                <div key={s.id} className={`${styles.subtask} ${s.done ? styles.subtaskDone : ""}`}>
                  <input
                    type="checkbox"
                    checked={s.done}
                    onChange={async (e) => {
                      await client.setSubtask(t.id, s.id, e.target.checked);
                      await load();
                    }}
                  />
                  <span>{s.title}</span>
                  <button
                    type="button"
                    className={styles.subtaskDel}
                    aria-label={strings.taskDelete}
                    onClick={async () => {
                      await client.deleteSubtask(t.id, s.id);
                      await load();
                    }}
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              ))}
            </div>
            <input
              className={styles.tdSubAdd}
              value={newSub}
              placeholder={strings.taskAddSubtask}
              onChange={(e) => setNewSub(e.target.value)}
              onKeyDown={async (e) => {
                if (e.key === "Enter" && newSub.trim()) {
                  await client.addSubtask(t.id, newSub.trim());
                  setNewSub("");
                  await load();
                  onChanged();
                }
              }}
            />
          </div>

          <div className={styles.tdSection}>
            <span className={styles.tdSectionLabel}>{strings.taskComments}</span>
            {data.comments.map((c) => (
              <div key={c.id} className={styles.comment}>
                <div className={styles.commentHead}>
                  <strong>{c.author}</strong>
                  <span>{new Date(c.createdAt).toLocaleString()}</span>
                </div>
                <div className={styles.commentBody}>{c.body}</div>
              </div>
            ))}
            <textarea
              className={styles.tdSubAdd}
              rows={2}
              value={newComment}
              placeholder={strings.taskAddComment}
              onChange={(e) => setNewComment(e.target.value)}
              onKeyDown={async (e) => {
                if (e.key === "Enter" && !e.shiftKey && newComment.trim()) {
                  e.preventDefault();
                  await client.addTaskComment(t.id, newComment.trim());
                  setNewComment("");
                  await load();
                }
              }}
            />
          </div>

          {data.activity.length > 0 && (
            <div className={styles.tdSection}>
              <span className={styles.tdSectionLabel}>{strings.taskActivity}</span>
              {data.activity.slice(0, 12).map((a, i) => (
                <div key={i} className={styles.activity}>
                  {a.actor} · {strings.taskActivityKind(a.kind)} · {new Date(a.createdAt).toLocaleString()}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
