// The "New task" modal: a proper create form over the real task fields (name,
// project, assignee, due date, priority, description, subtasks). Everything it
// sets is persisted in one createTask call (plus a subtask call per checklist
// line). "Create another" keeps it open and clears it for fast entry.
import { useState } from "react";
import type { FormEvent } from "react";
import { FolderClosed, Plus, SquareCheckBig, Trash2, User, X } from "lucide-react";

import { strings } from "../i18n";
import { useJmapClient, type TaskPriority, type TaskProject } from "../jmap";
import { Button, DatePicker } from "../ds";
import styles from "./TasksModule.module.css";

interface Props {
  projects: TaskProject[];
  defaultProjectId?: string | undefined;
  defaultStatus?: string | undefined;
  onClose: () => void;
  onCreated: () => void;
}

const PRIOS: { key: TaskPriority; label: string; cls: string }[] = [
  { key: "low", label: "", cls: "prioDotLow" },
  { key: "medium", label: "", cls: "prioDotMedium" },
  { key: "high", label: "", cls: "prioDotHigh" },
];

export function NewTaskModal({ projects, defaultProjectId, defaultStatus, onClose, onCreated }: Props) {
  const client = useJmapClient();
  const personal = projects.find((p) => p.kind === "personal") ?? projects[0];
  const [name, setName] = useState("");
  const [projectId, setProjectId] = useState(defaultProjectId ?? personal?.id ?? "");
  const [assignee, setAssignee] = useState("");
  const [dueDate, setDueDate] = useState("");
  const [priority, setPriority] = useState<TaskPriority>("none");
  const [description, setDescription] = useState("");
  const [subtasks, setSubtasks] = useState<string[]>([]);
  const [createAnother, setCreateAnother] = useState(false);
  const [busy, setBusy] = useState(false);

  const prioLabel = (k: TaskPriority) =>
    k === "low"
      ? strings.taskPrioLow
      : k === "medium"
        ? strings.taskPrioMedium
        : strings.taskPrioHigh;

  function reset() {
    setName("");
    setAssignee("");
    setDueDate("");
    setPriority("none");
    setDescription("");
    setSubtasks([]);
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    const title = name.trim();
    if (title === "" || projectId === "" || busy) return;
    setBusy(true);
    try {
      const input: Parameters<typeof client.createTask>[0] = { projectId, title };
      if (description.trim() !== "") input.description = description.trim();
      if (assignee.trim() !== "") input.assignee = assignee.trim();
      if (dueDate !== "") input.dueAt = `${dueDate}T12:00:00Z`;
      if (priority !== "none") input.priority = priority;
      if (defaultStatus !== undefined && defaultStatus !== "todo") input.status = defaultStatus;
      const created = await client.createTask(input);
      for (const st of subtasks.map((s) => s.trim()).filter((s) => s !== "")) {
        await client.addSubtask(created.id, st);
      }
      onCreated();
      if (createAnother) reset();
      else onClose();
    } catch {
      /* leave the form up so nothing is lost */
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={styles.ntScrim} role="dialog" aria-modal="true" onMouseDown={onClose}>
      <form className={styles.ntModal} onSubmit={submit} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.ntHead}>
          <span className={styles.ntHeadIcon}>
            <SquareCheckBig size={20} />
          </span>
          <div className={styles.ntHeadText}>
            <h2>{strings.taskNew}</h2>
            <p>{strings.taskNewSubtitle}</p>
          </div>
          <button type="button" className={styles.ntClose} onClick={onClose} aria-label={strings.taskCancel}>
            <X size={18} />
          </button>
        </div>

        <div className={styles.ntBody}>
          <label className={styles.ntField}>
            <span className={styles.ntLabel}>
              {strings.taskColName} <span className={styles.ntReq}>*</span>
            </span>
            <input
              className={styles.ntTitle}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={strings.taskNamePlaceholder}
              autoFocus
              required
            />
          </label>

          <div className={styles.ntTwoCol}>
            <label className={styles.ntField}>
              <span className={styles.ntLabel}>{strings.taskColProject}</span>
              <span className={styles.ntControl}>
                <FolderClosed size={16} className={styles.ntControlIcon} />
                <select value={projectId} onChange={(e) => setProjectId(e.target.value)}>
                  {projects.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </span>
            </label>
            <label className={styles.ntField}>
              <span className={styles.ntLabel}>{strings.taskColAssignee}</span>
              <span className={styles.ntControl}>
                <User size={16} className={styles.ntControlIcon} />
                <input
                  value={assignee}
                  onChange={(e) => setAssignee(e.target.value)}
                  placeholder={strings.taskAssigneePlaceholder}
                  inputMode="email"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
                />
              </span>
            </label>
          </div>

          <div className={styles.ntTwoCol}>
            <div className={styles.ntField}>
              <span className={styles.ntLabel}>{strings.taskColDue}</span>
              <DatePicker value={dueDate} onChange={setDueDate} placeholder={strings.taskColDue} />
            </div>
            <div className={styles.ntField}>
              <span className={styles.ntLabel}>{strings.taskColPriority}</span>
              <div className={styles.ntPrios}>
                {PRIOS.map((p) => (
                  <button
                    key={p.key}
                    type="button"
                    className={`${styles.ntPrio} ${priority === p.key ? styles.ntPrioOn : ""}`}
                    data-prio={p.key}
                    onClick={() => setPriority((cur) => (cur === p.key ? "none" : p.key))}
                  >
                    <span className={`${styles.prioDot} ${styles[p.cls] ?? ""}`} aria-hidden />
                    {prioLabel(p.key)}
                  </button>
                ))}
              </div>
            </div>
          </div>

          <label className={styles.ntField}>
            <span className={styles.ntLabel}>{strings.taskDescription}</span>
            <textarea
              className={styles.ntTextarea}
              rows={3}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder={strings.taskDescriptionPlaceholder}
            />
          </label>

          <div className={styles.ntField}>
            <span className={styles.ntLabel}>{strings.taskSubtasks}</span>
            {subtasks.map((st, i) => (
              <span key={i} className={styles.ntSubtask}>
                <input
                  value={st}
                  onChange={(e) =>
                    setSubtasks((cur) => cur.map((v, j) => (j === i ? e.target.value : v)))
                  }
                  placeholder={strings.taskAddSubtask}
                  autoFocus
                />
                <button
                  type="button"
                  className={styles.ntSubDel}
                  onClick={() => setSubtasks((cur) => cur.filter((_, j) => j !== i))}
                  aria-label={strings.taskDelete}
                >
                  <Trash2 size={15} />
                </button>
              </span>
            ))}
            <button
              type="button"
              className={styles.ntAdd}
              onClick={() => setSubtasks((cur) => [...cur, ""])}
            >
              <Plus size={16} /> {strings.taskAddSubtask}
            </button>
          </div>
        </div>

        <div className={styles.ntFooter}>
          <label className={styles.ntAnother}>
            <input
              type="checkbox"
              checked={createAnother}
              onChange={(e) => setCreateAnother(e.target.checked)}
            />
            {strings.taskCreateAnother}
          </label>
          <div className={styles.ntFooterRight}>
            <button type="button" className={styles.ntCancel} onClick={onClose} disabled={busy}>
              {strings.taskCancel}
            </button>
            <Button type="submit" disabled={busy || name.trim() === "" || projectId === ""}>
              <Plus size={16} /> {strings.taskCreate}
            </Button>
          </div>
        </div>
      </form>
    </div>
  );
}
