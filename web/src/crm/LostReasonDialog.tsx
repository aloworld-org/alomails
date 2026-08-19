// Why a deal was lost (alo CRM, B2.08) — asked before the move is sent, so
// cancelling the question cancels the move and no request is made the server
// would only refuse.
//
// It is a **picker with a free-text field**, not a plain prompt and not a fixed
// list. The store takes any string up to 200 characters, so a closed vocabulary
// here would be a rule invented by a screen; but a reason nobody can answer in
// one click is a reason nobody enters, which is the failure mode the whole
// feature exists to prevent. The suggestions fill the field — they never
// replace it.
//
// The hook shape is what lets `moveDeal` stay the one place that knows a losing
// column needs a reason: a caller renders `dialog` and hands `ask` to the move,
// and the promise resolves with the reason or `null` when the user backed out.
import { useCallback, useRef, useState } from "react";
import { TrendingDown } from "lucide-react";

import { Chip, Field, Input } from "../ds";
import { strings } from "../i18n";
import { DialogFrame } from "./parts";
import type { CrmStage } from "./types";

/** The reasons a deal is usually lost, offered as one click each. Ordinary
 *  suggestions, in the interface language — never stored as codes, because the
 *  stored reason is what a human reads in the report next quarter. */
function suggestions(): string[] {
  return [
    strings.crmLostReasonPrice,
    strings.crmLostReasonTiming,
    strings.crmLostReasonCompetitor,
    strings.crmLostReasonBudget,
    strings.crmLostReasonNoDecision,
    strings.crmLostReasonNotAFit,
  ];
}

/** What a pending question is waiting on: the column being moved into, and the
 *  resolver of the promise the caller is awaiting. */
interface Pending {
  stage: CrmStage;
  settle: (reason: string | null) => void;
}

/**
 * The lost-reason question as a pair: `ask(stage)` opens it and resolves with
 * the trimmed reason (or `null` when cancelled), and `dialog` is what the
 * caller renders.
 */
export function useLostReason(): {
  ask: (stage: CrmStage) => Promise<string | null>;
  dialog: React.ReactNode;
} {
  const [pending, setPending] = useState<Pending | null>(null);
  // Held in a ref as well so a settle can never be lost to a re-render: the
  // promise a caller is awaiting must be settled exactly once, whatever the
  // component does around it.
  const open = useRef<Pending | null>(null);

  const ask = useCallback((stage: CrmStage) => {
    return new Promise<string | null>((resolve) => {
      const next = { stage, settle: resolve };
      open.current = next;
      setPending(next);
    });
  }, []);

  const settle = useCallback((reason: string | null) => {
    open.current?.settle(reason);
    open.current = null;
    setPending(null);
  }, []);

  return {
    ask,
    dialog:
      pending === null ? null : (
        <LostReasonDialog stage={pending.stage} onSettle={settle} />
      ),
  };
}

function LostReasonDialog({
  stage,
  onSettle,
}: {
  stage: CrmStage;
  onSettle: (reason: string | null) => void;
}) {
  const [reason, setReason] = useState("");
  const trimmed = reason.trim();
  return (
    <DialogFrame
      Icon={TrendingDown}
      title={strings.crmLostTitle}
      subtitle={strings.crmLostMessage(stage.name)}
      error={null}
      busy={false}
      // A blank reason cannot be submitted here, for the same reason the server
      // refuses one: it is not a reason.
      canSubmit={trimmed !== ""}
      submitLabel={strings.crmLostConfirm}
      onClose={() => onSettle(null)}
      onSubmit={() => onSettle(trimmed)}
    >
      {/* One click each, and they fill the field rather than replacing it —
          which is why they are chips that carry a pressed state rather than a
          set of radio buttons: the answer is still free text. */}
      <div className="flex flex-wrap gap-1.5">
        {suggestions().map((suggested) => (
          <Chip
            key={suggested}
            pressed={trimmed === suggested}
            onClick={() => setReason(suggested)}
          >
            {suggested}
          </Chip>
        ))}
      </div>
      <Field label={strings.crmLostReasonLabel}>
        {(control) => (
          <Input
            {...control}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder={strings.crmLostPlaceholder}
            autoFocus
          />
        )}
      </Field>
    </DialogFrame>
  );
}
