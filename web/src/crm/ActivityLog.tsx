// A deal's log: what was said and done, by when it **happened** rather than
// when somebody typed it up — so a call written up in the evening still reads
// in the right place.
//
// Two rules the panel shows rather than explains: an entry is written once (a
// correction is another note, so there is no edit), and it is removed only by
// the colleague who wrote it. That second one is a `403` from the server, and
// it is shown as the sentence the server wrote rather than hidden as a `404`:
// the log is readable tenant-wide, so pretending the entry is not there would
// be theatre.
import { useCallback, useEffect, useState } from "react";
import { MessageSquare, Trash2 } from "lucide-react";

import { Button, IconButton, Select } from "../ds";
import { strings } from "../i18n";
import { crmMessage, useCrmApi } from "./api";
import { kindLabel, momentLabel } from "./format";
import { ErrorBanner } from "./parts";
import type { ActivityKind, DealActivity } from "./types";
import styles from "./CrmModule.module.css";

const KINDS: ActivityKind[] = ["note", "call", "meeting"];

export function ActivityLog({ dealId }: { dealId: string }) {
  const api = useCrmApi();
  const [entries, setEntries] = useState<DealActivity[]>([]);
  const [kind, setKind] = useState<ActivityKind>("note");
  const [body, setBody] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setEntries(await api.activities(dealId));
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
      // `happenedAt` is deliberately not sent: an entry nobody dated happened
      // now, and "now" is the server's clock, not a browser's.
      await api.addActivity(dealId, { kind, body });
      setBody("");
      await load();
    } catch (err) {
      setError(crmMessage(err, strings.crmSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function remove(id: string) {
    try {
      await api.deleteActivity(id);
      await load();
    } catch (err) {
      setError(crmMessage(err, strings.crmDeleteFailed));
    }
  }

  return (
    <section className="rounded-xl border border-subtle bg-surface p-5 shadow-sm">
      <h3 className="m-0 flex items-center gap-2 text-sm font-semibold text-primary">
        <span className="grid size-8 place-items-center rounded-lg bg-accent-soft text-accent"><MessageSquare size={16} /></span>
        {strings.crmActivityTitle}
      </h3>

      {error !== null && <ErrorBanner message={error} />}

      <form
        className="mt-4 grid grid-cols-[9rem_minmax(0,1fr)_auto] items-start gap-3 max-md:grid-cols-1"
        onSubmit={(e) => {
          e.preventDefault();
          if (!busy && body.trim() !== "") void add();
        }}
      >
        <Select
          value={kind}
          onChange={(e) => setKind(e.target.value as ActivityKind)}
          aria-label={strings.crmActivityKind}
        >
          {KINDS.map((k) => (
            <option key={k} value={k}>
              {kindLabel(k)}
            </option>
          ))}
        </Select>
        <textarea
          className="min-h-24 w-full resize-y rounded-xl border border-default bg-surface !px-4 py-3 text-sm leading-5 text-primary placeholder:text-tertiary focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/10"
          value={body}
          onChange={(e) => setBody(e.target.value)}
          rows={2}
          placeholder={strings.crmActivityPlaceholder}
          aria-label={strings.crmActivityPlaceholder}
        />
        <Button type="submit" disabled={busy || body.trim() === ""}>
          {strings.crmActivityAdd}
        </Button>
      </form>

      {entries.length === 0 ? (
        <p className="mb-0 mt-4 rounded-lg bg-raised/40 px-4 py-3 text-sm text-secondary">{strings.crmActivityEmpty}</p>
      ) : (
        <ul className={`${styles.entries} mt-4`} aria-label={strings.crmActivityTitle}>
          {entries.map((entry) => (
            <li key={entry.id} className={styles.entry}>
              <div className={styles.entryHead}>
                <span className={styles.entryKind}>
                  {kindLabel(entry.kind)}
                </span>
                <span className={styles.entryWhen}>
                  {momentLabel(entry.happenedAt)}
                </span>
                <span className={styles.cardSpacer} />
                <IconButton
                  label={strings.crmActivityDelete}
                  icon={<Trash2 size={14} />}
                  onClick={() => void remove(entry.id)}
                  title={strings.crmActivityDelete}
                />
              </div>
              <p className={styles.entryBody}>{entry.body}</p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
