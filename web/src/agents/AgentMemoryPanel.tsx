// What an agent remembers — per agent, per conversation (ADR 0057 §6).
//
// This panel is the transparency half of channel memory: everything an agent
// has retained from a conversation, readable by everyone who can read the
// conversation itself. Nothing here is secret from the room — that is the
// point, and the sentence at the top says so.
//
// Forgetting is narrower than reading, and the server decides it: the room's
// owner (either side of a one-to-one) may forget anything, the author of the
// words a fact was learned from may forget that fact. The button appears only
// where the server said it would be honoured (`canForget`), so nobody is
// offered a refusal.
import { useCallback, useEffect, useState } from "react";
import { X } from "lucide-react";

import { chatMessage, useChatApi } from "../chat/api";
import type { Agent, AgentMemory } from "../chat/types";
import { Button, IconButton } from "../ds";
import { strings } from "../i18n";

export function AgentMemoryPanel({
  channel,
  agent,
  aboutYou,
  onClose,
}: {
  channel: string;
  agent: Agent;
  /** The conversation is the caller's own one-to-one with this agent, so the
   *  facts are about them and visible to them alone. */
  aboutYou: boolean;
  onClose: () => void;
}) {
  const api = useChatApi();
  const [memories, setMemories] = useState<AgentMemory[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setMemories(await api.agentMemories(channel, agent.id));
    } catch (failure) {
      setError(chatMessage(failure, strings.agentMemoryLoadFailed));
    }
  }, [api, channel, agent.id]);

  useEffect(() => {
    void load();
  }, [load]);

  async function forget(memory: AgentMemory) {
    setBusy(memory.id);
    setError(null);
    try {
      await api.forgetMemory(memory.id);
      await load();
    } catch (failure) {
      setError(chatMessage(failure, strings.agentMemoryForgetFailed));
    } finally {
      setBusy(null);
    }
  }

  const when = (at: string): string =>
    new Date(at).toLocaleDateString(undefined, {
      day: "numeric",
      month: "short",
      year: "numeric",
    });

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-overlay p-4"
      role="dialog"
      aria-modal="true"
      aria-label={strings.agentMemoryTitle(agent.handle)}
    >
      <div className="flex max-h-full min-h-0 w-full max-w-md flex-col overflow-hidden rounded-lg border border-subtle bg-surface shadow-lg">
        <header className="flex items-center justify-between gap-2 border-b border-subtle p-3">
          <h2 className="m-0 text-sm font-semibold text-primary">
            {strings.agentMemoryTitle(agent.handle)}
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
            {aboutYou ? strings.agentMemoryAboutYou : strings.agentMemoryShared}
          </p>

          {memories !== null && memories.length === 0 && (
            <p className="m-0 text-sm text-tertiary">{strings.agentMemoryEmpty}</p>
          )}

          {memories !== null && memories.length > 0 && (
            <ul className="m-0 flex list-none flex-col gap-1 p-0">
              {memories.map((memory) => (
                <li
                  key={memory.id}
                  className="group flex min-h-10 items-center gap-2 rounded-sm px-2 py-1.5 hover:bg-raised"
                >
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="text-sm text-primary">{memory.fact}</span>
                    <span className="truncate text-xs text-tertiary">
                      {memory.learnedFrom === "explicit"
                        ? strings.agentMemoryExplicit
                        : strings.agentMemoryFromTurn}
                      {" · "}
                      {when(memory.createdAt)}
                    </span>
                  </span>
                  {memory.canForget && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => void forget(memory)}
                      disabled={busy === memory.id}
                      aria-label={strings.agentMemoryForgetFact(memory.fact)}
                    >
                      {strings.agentMemoryForget}
                    </Button>
                  )}
                </li>
              ))}
            </ul>
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
