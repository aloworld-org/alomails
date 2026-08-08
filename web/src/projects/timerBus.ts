// One notification: "the running timer changed".
//
// The clock is written from two places that are not in one another's component
// tree — the rail widget (`TimerWidget`) and the module's own controls — and a
// widget that only read on mount would go on showing a clock somebody stopped
// on the Projects screen. Lifting the timer into a shared context would put a
// Projects concern into the app shell for every product, including the mail-only
// one that has no Projects at all.
//
// So: a DOM event on `window`, which is the one channel two independent React
// trees on one page already share. It carries no payload — the answer is always
// re-read from the server, which is the only thing that knows whether the write
// actually landed.
const TIMER_CHANGED = "alo:projects-timer-changed";

/** Says the running timer may have changed. Called after any start or stop. */
export function announceTimerChanged(): void {
  window.dispatchEvent(new Event(TIMER_CHANGED));
}

/** Runs `listener` whenever the running timer may have changed. Returns the
 *  unsubscribe an effect cleans up with. */
export function onTimerChanged(listener: () => void): () => void {
  window.addEventListener(TIMER_CHANGED, listener);
  return () => window.removeEventListener(TIMER_CHANGED, listener);
}
