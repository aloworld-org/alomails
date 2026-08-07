// Moving a card — the one CRM action with a rule the user has to answer for, in
// one place because the board and the drawer both do it.
//
// A losing column requires a reason and every other column refuses one
// (`docs/design/crm.md` § Won, lost, and reopened). So the reason is asked for
// BEFORE the move is sent: cancelling the question cancels the move, and no
// request is made that the server would only refuse. Nothing else about the
// move is decided here — which columns are losing ones is the server's `isLost`
// flag, and the history row and closing snapshot are written in its own
// transaction.
//
// **How** the reason is asked is the caller's (`useLostReason`, B2.08): a
// screen renders the picker, this function only knows that a losing column has
// a question attached to it.
import type { CrmApi } from "./api";
import type { CrmDeal, CrmStage } from "./types";

/** Asks the user why a deal moving into `stage` was lost. Resolves with the
 *  reason, or `null` when they backed out. */
export type AskLostReason = (stage: CrmStage) => Promise<string | null>;

/**
 * Moves `dealId` into `stage`, asking for a lost reason first when the column
 * needs one.
 *
 * Answers the stored deal, or `null` when the user cancelled the question — the
 * caller then leaves the card where it was. Failures are thrown, so the screen
 * that started the move reports them the way it reports every other failure.
 */
export async function moveDeal(
  api: CrmApi,
  ask: AskLostReason,
  dealId: string,
  stage: CrmStage,
  position?: number,
): Promise<CrmDeal | null> {
  let lostReason: string | undefined;
  if (stage.isLost) {
    const answer = await ask(stage);
    // Cancelled, or nothing typed: the deal stays where it is. A reason that is
    // optional is a reason nobody enters, which is why the server refuses a
    // blank one too.
    if (answer === null || answer.trim() === "") return null;
    lostReason = answer.trim();
  }
  return api.moveDeal(dealId, {
    stageId: stage.id,
    ...(position === undefined ? {} : { position }),
    ...(lostReason === undefined ? {} : { lostReason }),
  });
}
