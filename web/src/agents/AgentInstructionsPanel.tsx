// The standing-instruction cards of a room (ADR 0057 §7, queue item A7.2).
//
// A person asks once, in advance: the card shows the instruction in the
// author's words, the agent that runs it, the trigger, and when it next
// fires. Everyone who can read the room reads the cards — an agent acting on
// a clock in a shared room is not one person's private arrangement.
//
// Cancel is narrower than reading, and the server decides it: the author's
// own brake, or the room owner's. The button appears only where the server
// said it would be honoured (`canCancel`), so nobody is offered a refusal.
//
// The form stands a *scheduled* instruction up. Event-triggered cards are
// rendered and cancellable here, but not created: the events the design
// names as examples (`invoice.overdue`) have no emitter yet, and a dropdown
// of raw registry verbs would be a surface that reads as a debugger.
import { useCallback, useEffect, useState } from "react";
import { X } from "lucide-react";

import { chatMessage, useChatApi } from "../chat/api";
import type { Agent, AgentInstruction } from "../chat/types";
import { Button, Field, IconButton, Input, MODAL_BACKDROP_CLASS, Select } from "../ds";
import { strings } from "../i18n";

/** The schedules the form offers. The server's floor is one hour. */
const INTERVALS: { minutes: number; label: () => string }[] = [
  { minutes: 60, label: () => strings.agentInstructionOptionHourly },
  { minutes: 240, label: () => strings.agentInstructionOption4Hours },
  { minutes: 1440, label: () => strings.agentInstructionOptionDaily },
  { minutes: 10080, label: () => strings.agentInstructionOptionWeekly },
];

/** The trigger, said as a sentence — the card's own words for its clock. */
function triggerLine(card: AgentInstruction): string {
  if (card.trigger.kind === "event") {
    return strings.agentInstructionOnEvent(card.trigger.event);
  }
  const minutes = card.trigger.everyMinutes;
  if (minutes === 60) return strings.agentInstructionHourly;
  if (minutes === 1440) return strings.agentInstructionDaily;
  if (minutes === 10080) return strings.agentInstructionWeekly;
  if (minutes % 60 === 0) {
    return strings.agentInstructionEveryHours(minutes / 60);
  }
  return strings.agentInstructionEveryMinutes(minutes);
}

export function AgentInstructionsPanel({
  channel,
  onClose,
}: {
  channel: string;
  onClose: () => void;
}) {
  const api = useChatApi();
  const [cards, setCards] = useState<AgentInstruction[] | null>(null);
  // The room's awake agents, for choosing who runs a new instruction. The
  // server refuses a retired or absent one, so only these are offered.
  const [agents, setAgents] = useState<Agent[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [agentId, setAgentId] = useState("");
  const [text, setText] = useState("");
  const [everyMinutes, setEveryMinutes] = useState(60);
  const [adding, setAdding] = useState(false);

  const load = useCallback(async () => {
    try {
      const [instructions, here] = await Promise.all([
        api.channelInstructions(channel),
        api.channelAgents(channel),
      ]);
      setCards(instructions);
      setAgents(here.filter((agent) => !agent.disabled));
    } catch (failure) {
      setError(chatMessage(failure, strings.agentInstructionsLoadFailed));
    }
  }, [api, channel]);

  useEffect(() => {
    void load();
  }, [load]);

  async function add() {
    if (agentId === "" || text.trim() === "") return;
    setAdding(true);
    setError(null);
    try {
      await api.createInstruction(channel, {
        agentId,
        text: text.trim(),
        trigger: { kind: "schedule", everyMinutes },
      });
      setText("");
      await load();
    } catch (failure) {
      setError(chatMessage(failure, strings.agentInstructionCreateFailed));
    } finally {
      setAdding(false);
    }
  }

  async function cancel(card: AgentInstruction) {
    setBusy(card.id);
    setError(null);
    try {
      await api.cancelInstruction(card.id);
      await load();
    } catch (failure) {
      setError(chatMessage(failure, strings.agentInstructionCancelFailed));
    } finally {
      setBusy(null);
    }
  }

  const when = (at: string): string =>
    new Date(at).toLocaleString(undefined, {
      day: "numeric",
      month: "short",
      hour: "2-digit",
      minute: "2-digit",
    });

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-overlay p-4 ${MODAL_BACKDROP_CLASS}`}
      role="dialog"
      aria-modal="true"
      aria-label={strings.agentInstructionsTitle}
    >
      <div className="flex max-h-full min-h-0 w-full max-w-md flex-col overflow-hidden rounded-lg border border-subtle bg-surface shadow-lg">
        <header className="flex items-center justify-between gap-2 border-b border-subtle p-3">
          <h2 className="m-0 text-sm font-semibold text-primary">
            {strings.agentInstructionsTitle}
          </h2>
          <IconButton
            onClick={onClose}
            label={strings.chatClose}
            icon={<X size={16} />}
            size="sm"
          />
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto p-3">
          <p className="m-0 mb-3 text-xs text-tertiary">
            {strings.agentInstructionsIntro}
          </p>

          {cards !== null && cards.length === 0 && (
            <p className="m-0 text-sm text-tertiary">
              {strings.agentInstructionsEmpty}
            </p>
          )}

          {cards !== null && cards.length > 0 && (
            <ul className="m-0 flex list-none flex-col gap-2 p-0">
              {cards.map((card) => (
                <li
                  key={card.id}
                  className="flex flex-col gap-1 rounded-md border border-subtle p-2.5"
                >
                  <div className="flex items-center gap-2">
                    <span className="min-w-0 flex-1 truncate text-xs font-semibold text-accent">
                      @{card.agentHandle}
                    </span>
                    {card.canCancel && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => void cancel(card)}
                        disabled={busy === card.id}
                        aria-label={strings.agentInstructionCancelThis(card.text)}
                      >
                        {strings.agentInstructionCancel}
                      </Button>
                    )}
                  </div>
                  <p className="m-0 text-sm text-primary">{card.text}</p>
                  <p className="m-0 text-xs text-tertiary">
                    {triggerLine(card)}
                    {!card.paused && card.nextRun !== null && (
                      <> · {strings.agentInstructionNextRun(when(card.nextRun))}</>
                    )}
                    {card.author !== null && (
                      <> · {strings.agentInstructionAskedBy(card.author)}</>
                    )}
                  </p>
                  {card.paused && (
                    // The design's own answer to "instruction's author gone":
                    // paused, and the card says so.
                    <p className="m-0 text-xs font-semibold text-accent">
                      {strings.agentInstructionPaused}
                    </p>
                  )}
                </li>
              ))}
            </ul>
          )}

          {agents.length > 0 && (
            <form
              className="mt-4 flex flex-col gap-2 border-t border-subtle pt-3"
              onSubmit={(submit) => {
                submit.preventDefault();
                void add();
              }}
            >
              <Field label={strings.agentInstructionAgentLabel}>
                {(control) => (
                  <Select
                    id={control.id}
                    aria-describedby={control["aria-describedby"]}
                    value={agentId}
                    onChange={(change) => setAgentId(change.target.value)}
                    placeholder={strings.agentInstructionAgentLabel}
                    required
                    fullWidth
                  >
                    {agents.map((agent) => (
                      <option key={agent.id} value={agent.id}>
                        @{agent.handle}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
              <Field label={strings.agentInstructionTextLabel}>
                {(control) => (
                  <Input
                    id={control.id}
                    aria-describedby={control["aria-describedby"]}
                    value={text}
                    onChange={(change) => setText(change.target.value)}
                    placeholder={strings.agentInstructionTextPlaceholder}
                    maxLength={400}
                    required
                  />
                )}
              </Field>
              <Field label={strings.agentInstructionScheduleLabel}>
                {(control) => (
                  <Select
                    id={control.id}
                    aria-describedby={control["aria-describedby"]}
                    value={String(everyMinutes)}
                    onChange={(change) =>
                      setEveryMinutes(Number(change.target.value))
                    }
                    fullWidth
                  >
                    {INTERVALS.map((interval) => (
                      <option key={interval.minutes} value={interval.minutes}>
                        {interval.label()}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
              <div className="flex justify-end">
                <Button type="submit" disabled={adding}>
                  {strings.agentInstructionAdd}
                </Button>
              </div>
            </form>
          )}

          {error !== null && (
            <p className="mt-3 text-sm text-accent" role="alert">
              {error}
            </p>
          )}
        </div>

        <footer className="flex justify-end border-t border-subtle p-3">
          <Button variant="ghost" onClick={onClose}>
            {strings.chatClose}
          </Button>
        </footer>
      </div>
    </div>
  );
}
