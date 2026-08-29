// The Agenda settings surface for a person's working schedule: which days,
// the daily window, and the time zone the window follows. What it edits is
// what free/busy answers with as "outside hours" when colleagues schedule
// with this person — the dialog says so, so the setting explains its effect.
import { useEffect, useMemo, useState } from "react";
import { X } from "lucide-react";

import { getLocale, strings } from "../i18n";
import { Button, Field, IconButton, Input, Modal, Select } from "../ds";
import { useJmapClient, type WorkingHours } from "../jmap";
import styles from "./AgendaModule.module.css";

interface Props {
  onClose: () => void;
}

/** ISO weekday numbers in display order, Monday first. */
const WEEK: number[] = [1, 2, 3, 4, 5, 6, 7];

export function WorkingHoursDialog({ onClose }: Props) {
  const client = useJmapClient();
  const locale = getLocale();
  const [days, setDays] = useState<ReadonlySet<number>>(new Set());
  const [start, setStart] = useState("09:00");
  const [end, setEnd] = useState("17:00");
  // "" = the person's own zone (the wire's null).
  const [zone, setZone] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const hours = await client.workingHours();
        if (cancelled) return;
        setDays(new Set(hours.days));
        setStart(hours.start);
        setEnd(hours.end);
        setZone(hours.zone ?? "");
        setLoaded(true);
      } catch {
        if (!cancelled) setError(strings.agendaWorkingHoursLoadError);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client]);

  // Weekday names in the viewer's language, keyed by ISO number. Any week
  // containing a known Monday does; 2026-06-01 is one.
  const dayName = useMemo(() => {
    const fmt = new Intl.DateTimeFormat(locale, { weekday: "short" });
    const names = new Map<number, string>();
    for (const day of WEEK) {
      names.set(day, fmt.format(new Date(Date.UTC(2026, 5, day))));
    }
    return names;
  }, [locale]);

  // The browser knows the IANA zone list; no server round-trip needed.
  const zones = useMemo(() => Intl.supportedValuesOf("timeZone"), []);

  function toggleDay(day: number) {
    setDays((prev) => {
      const next = new Set(prev);
      if (next.has(day)) next.delete(day);
      else next.add(day);
      return next;
    });
  }

  async function save() {
    if (end <= start) {
      setError(strings.agendaWorkHoursOrder);
      return;
    }
    setBusy(true);
    setError(null);
    const hours: WorkingHours = {
      days: WEEK.filter((day) => days.has(day)),
      start,
      end,
      zone: zone === "" ? null : zone,
    };
    try {
      await client.setWorkingHours(hours);
      onClose();
    } catch {
      setError(strings.agendaWorkingHoursError);
      setBusy(false);
    }
  }

  return (
    <Modal
      title={strings.agendaWorkingHours}
      onClose={onClose}
      actions={
        <IconButton
          label={strings.agendaClose}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          <div className={styles.footSpacer} />
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {strings.agendaClose}
          </Button>
          <Button onClick={() => void save()} disabled={busy || !loaded}>
            {strings.agendaSave}
          </Button>
        </>
      }
    >
      <p className={styles.fieldHint}>{strings.agendaWorkingHoursHint}</p>

      <fieldset className={styles.workDays}>
        <legend className={styles.emLabel}>{strings.agendaWorkingDays}</legend>
        <div className={styles.workDayRow}>
          {WEEK.map((day) => (
            <button
              key={day}
              type="button"
              className={`${styles.workDay} ${days.has(day) ? styles.workDayOn : ""}`}
              aria-pressed={days.has(day)}
              onClick={() => toggleDay(day)}
              disabled={!loaded}
            >
              {dayName.get(day)}
            </button>
          ))}
        </div>
      </fieldset>

      <div className={styles.shareForm}>
        <Field label={strings.agendaWorkStart}>
          {(control) => (
            <Input
              {...control}
              type="time"
              value={start}
              onChange={(e) => setStart(e.target.value)}
              disabled={!loaded}
            />
          )}
        </Field>
        <Field label={strings.agendaWorkEnd}>
          {(control) => (
            <Input
              {...control}
              type="time"
              value={end}
              onChange={(e) => setEnd(e.target.value)}
              disabled={!loaded}
            />
          )}
        </Field>
        <Field label={strings.agendaWorkZone}>
          {(control) => (
            <Select
              {...control}
              value={zone}
              onChange={(e) => setZone(e.target.value)}
              disabled={!loaded}
            >
              <option value="">{strings.agendaWorkZoneMine}</option>
              {zones.map((name) => (
                <option key={name} value={name}>
                  {name.replace(/_/g, " ")}
                </option>
              ))}
            </Select>
          )}
        </Field>
      </div>

      {error !== null && (
        <p className={styles.modalError} role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
