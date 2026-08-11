// One notification: "something waiting for a decision was decided".
//
// The count in the rail and the inbox that empties it are two independent React
// trees on one page (the widget is declared by the product surface; the view is
// a route inside the HR module), and a badge that only read on mount would go
// on claiming six things are waiting after somebody cleared them.
//
// The same channel `projects/timerBus.ts` opened, for the same reason and with
// the same rule: **it carries no payload.** The new count is always re-read
// from the three queues, because only the server knows which decisions actually
// landed — a badge that decremented itself would be a number nobody could
// reconcile with the list under it.
const APPROVALS_CHANGED = "alo:approvals-changed";

/** Says something in an approval queue may have moved. Called after any
 *  decision, whichever screen took it. */
export function announceApprovalsChanged(): void {
  window.dispatchEvent(new Event(APPROVALS_CHANGED));
}

/** Runs `listener` whenever an approval queue may have changed. Returns the
 *  unsubscribe an effect cleans up with. */
export function onApprovalsChanged(listener: () => void): () => void {
  window.addEventListener(APPROVALS_CHANGED, listener);
  return () => window.removeEventListener(APPROVALS_CHANGED, listener);
}
