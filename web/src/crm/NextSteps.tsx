// What happens next on a deal — as a **real task**, not a CRM-only reminder.
//
// A next step is created in the tasks module with the deal as its source
// (ADR 0021), so it lands in the list its owner actually opens tomorrow morning
// and is ticked off there. Two to-do lists in one workspace is how a CRM
// becomes the system nobody updates (`docs/design/crm.md`), so this panel shows
// the task and hands off to Tasks rather than growing a second one.
//
// It shows what **this** reader may see: the steps on team projects, plus
// anything assigned to them. A colleague's private list is not widened by one
// row because a deal is tenant-wide — the same asymmetry a linked conversation
// has, and the server's rule, not a second one invented here.
import { useCallback, useEffect, useState } from "react";
import { CalendarDays, CheckSquare, ExternalLink } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { Button, Input } from "../ds";
import { strings } from "../i18n";
import type { Task } from "../jmap";
import { crmMessage, useCrmApi } from "./api";
import { momentLabel } from "./format";
import { ErrorBanner } from "./parts";
import styles from "./CrmModule.module.css";

export function NextSteps({ dealId }: { dealId: string }) {
  const api = useCrmApi();
  const navigate = useNavigate();
  const [steps, setSteps] = useState<Task[]>([]);
  const [title, setTitle] = useState("");
  const [due, setDue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setSteps(await api.nextSteps(dealId));
      setError(null);
    } catch (err) {
      setError(crmMessage(err, strings.crmLoadFailed));
    }
  }, [api, dealId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function add() {
    setBusy(true);
    try {
      // A task's due date is an instant everywhere in Tasks, and the picker
      // gives a day: it is sent as that day's start in the browser's own zone,
      // which is the zone the person picking it is standing in.
      const dueAt =
        due === "" ? undefined : new Date(`${due}T00:00`).toISOString();
      await api.addNextStep(dealId, {
        title,
        ...(dueAt === undefined ? {} : { dueAt }),
      });
      setTitle("");
      setDue("");
      await load();
    } catch (err) {
      setError(crmMessage(err, strings.crmSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={styles.panel}>
      <h3 className={styles.panelTitle}>
        <CheckSquare size={15} /> {strings.crmNextStepsTitle}
      </h3>

      {error !== null && <ErrorBanner message={error} />}

      <form
        className={styles.composer}
        onSubmit={(e) => {
          e.preventDefault();
          if (!busy && title.trim() !== "") void add();
        }}
      >
        {/* The step takes the room the row has left; the day takes its own. */}
        <Input
          className="flex-1 basis-[200px]"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={strings.crmNextStepPlaceholder}
          aria-label={strings.crmNextStepPlaceholder}
        />
        <Input
          className="flex-none basis-40"
          type="date"
          value={due}
          onChange={(e) => setDue(e.target.value)}
          aria-label={strings.crmNextStepDue}
        />
        <Button type="submit" disabled={busy || title.trim() === ""}>
          {strings.crmNextStepAdd}
        </Button>
      </form>

      {steps.length === 0 ? (
        <p className={styles.panelEmpty}>{strings.crmNextStepsEmpty}</p>
      ) : (
        <ul className={styles.entries} aria-label={strings.crmNextStepsTitle}>
          {steps.map((step) => (
            <li key={step.id} className={styles.entry}>
              <div className={styles.entryHead}>
                <span className={styles.entryKind}>{step.title}</span>
                <span className={styles.cardSpacer} />
                {step.dueAt !== null && (
                  <span className={styles.entryWhen}>
                    <CalendarDays size={13} /> {momentLabel(step.dueAt)}
                  </span>
                )}
              </div>
              <button
                type="button"
                className={styles.linkAction}
                onClick={() =>
                  navigate(`/tasks?open=${encodeURIComponent(step.id)}`)
                }
              >
                <ExternalLink size={13} /> {strings.crmOpenInTasks}
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
