// The running-timer widget in the rail — the one Projects component that lives
// outside the module, because a timer you cannot see from your inbox is a timer
// you forget to stop.
//
// It is registered through the product surface (`web/src/product`), not
// imported by the shell: Projects is a workspace module and the standalone mail
// product must not grow a dependency on it. The rail renders whatever widgets
// its surface declares and knows nothing about clocks.
//
// Three rules:
//
// - **It polls nothing when no timer runs.** One read on mount and one after
//   any write; the ticking display is local arithmetic over a stored instant,
//   not a request per second.
// - **Stopping is the server's write.** The minutes that reach the entry are
//   counted there when the clock is stopped; the number ticking here is elapsed
//   time for a human to read, and it is never what is billed.
// - **The day is the browser's, in the browser's zone.** A clock started at
//   23:50 and stopped at 00:30 belongs to the day it started, and the server
//   falls back to exactly that when no day is stated — so this sends none.
import { useCallback, useEffect, useState } from "react";
import { Square, Timer } from "lucide-react";
import { Link } from "react-router-dom";

import { strings } from "../i18n";
import { projectsMessage, useProjectsApi } from "./api";
import { durationLabel, elapsedMinutes } from "./format";
import { announceTimerChanged, onTimerChanged } from "./timerBus";
import type { RunningTimer } from "./types";
const styles = {
  wrap: "flex w-full flex-col items-center gap-1",
  widget: "flex w-full flex-col items-center gap-1 rounded-md bg-rail-active px-1.5 py-2 text-on-rail !no-underline hover:bg-rail-hover hover:!no-underline",
  elapsed: "flex items-center gap-1 text-sm font-semibold tabular-nums",
  dot: "h-1.5 w-1.5 shrink-0 rounded-full bg-accent",
  project: "max-w-full overflow-hidden text-ellipsis whitespace-nowrap text-xs text-on-rail-muted",
  stop: "mt-0.5 inline-flex items-center gap-1 rounded-full bg-accent px-3 py-1 text-xs font-medium text-on-accent hover:bg-accent-hover disabled:opacity-60",
} as const;

/** How often the elapsed display is recomputed. A minute's resolution needs no
 *  faster tick, and a second's would repaint the rail 60 times for nothing. */
const TICK_MS = 15_000;

export function TimerWidget() {
  const api = useProjectsApi();
  const [timer, setTimer] = useState<RunningTimer | null>(null);
  const [projectName, setProjectName] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const running = await api.timer();
      setTimer(running);
      setError(null);
      if (running === null) {
        setProjectName(null);
        return;
      }
      // The engagement's name, so the rail says what is running rather than an
      // id. A failure here leaves the clock visible without a name — a widget
      // that vanished because a second read failed would lose the stop button.
      try {
        setProjectName((await api.project(running.projectId)).name);
      } catch {
        setProjectName(null);
      }
    } catch {
      // Not signed in yet, or the surface is unreachable: no clock is the
      // honest state of the rail, and this is not a failure to shout about.
      setTimer(null);
    }
  }, [api]);

  useEffect(() => {
    void load();
    // A clock started or stopped on the Projects screen is a clock this widget
    // has to redraw, and the two live in different React trees.
    return onTimerChanged(() => void load());
  }, [load]);

  useEffect(() => {
    if (timer === null) return;
    const tick = window.setInterval(() => setNow(Date.now()), TICK_MS);
    return () => window.clearInterval(tick);
  }, [timer]);

  async function stop() {
    setBusy(true);
    try {
      await api.stopTimer();
      setTimer(null);
      setProjectName(null);
      setError(null);
      announceTimerChanged();
    } catch (err) {
      setError(projectsMessage(err, strings.projectsStopFailed));
    } finally {
      setBusy(false);
    }
  }

  // Nothing running is the ordinary state of a workspace: the rail shows
  // nothing at all rather than an empty clock.
  if (timer === null) return null;

  // A clock in its first minute reads "0m", not the dash an empty cell shows:
  // a timer that says "—" for sixty seconds reads as a timer that is broken.
  const minutes = elapsedMinutes(timer.startedAt, now);
  const elapsed = minutes === 0 ? strings.projectsMinutesShort(0) : durationLabel(minutes);
  return (
    <div className={styles.wrap}>
      <Link
        to="/projects/week"
        className={styles.widget}
        title={error ?? projectName ?? strings.projectsTimerRunning}
      >
        <span className={styles.elapsed}>
          <span className={styles.dot} aria-hidden="true" />
          <Timer size={13} aria-hidden="true" />
          {elapsed}
        </span>
        <span className={styles.project}>{projectName ?? strings.projectsTimerRunning}</span>
      </Link>
      <button
        type="button"
        className={styles.stop}
        disabled={busy}
        onClick={() => void stop()}
        aria-label={strings.projectsStopTimer}
      >
        <Square size={11} aria-hidden="true" />
        {strings.projectsStop}
      </button>
    </div>
  );
}
