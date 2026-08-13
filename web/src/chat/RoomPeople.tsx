// Who is in a room — the people, and the agents.
//
// This exists because an agent could be *added* to a room only through the
// API: the one thing that makes alo Chat different from every other chat was
// unreachable from the interface. A feature nobody can turn on is a feature
// nobody has.
//
// People and agents are listed together, because from inside a conversation
// the distinction that matters is "who is here", not "which kind of thing".
// They are still visibly different — an agent is marked, never avatared, so
// nobody mistakes one for a colleague.
import { useCallback, useEffect, useState } from "react";
import { Sparkles, UserMinus, UserPlus, X } from "lucide-react";

import { Avatar, Button, IconButton } from "../ds";
import { strings } from "../i18n";
import { chatMessage, useChatApi } from "./api";
import type { Agent, ChannelDetail } from "./types";

const sectionClass = "mb-2 mt-3 first:mt-0 text-xs font-semibold uppercase tracking-wide text-tertiary";
const listClass = "m-0 flex list-none flex-col gap-1 p-0";
const rowClass = "group flex min-h-10 items-center gap-2 rounded-sm px-2 hover:bg-raised";
const whoClass = "flex min-w-0 flex-1 flex-col";

export function RoomPeople({
  channel,
  onClose,
  onChanged,
}: {
  channel: string;
  onClose: () => void;
  /** The room's cast changed, so the conversation should refetch. */
  onChanged: () => void;
}) {
  const api = useChatApi();
  const [detail, setDetail] = useState<ChannelDetail | null>(null);
  const [here, setHere] = useState<Agent[]>([]);
  const [available, setAvailable] = useState<Agent[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [room, inRoom, all] = await Promise.all([
        api.channel(channel),
        api.channelAgents(channel),
        api.agents(),
      ]);
      setDetail(room);
      setHere(inRoom);
      // Only ones not already here, and never a retired one: the server
      // refuses those, so offering them would be an invitation to a refusal.
      setAvailable(
        all.filter((a) => !a.disabled && !inRoom.some((h) => h.id === a.id)),
      );
    } catch (failure) {
      setError(chatMessage(failure, strings.chatLoadFailed));
    }
  }, [api, channel]);

  useEffect(() => {
    void load();
  }, [load]);

  async function addAgent(agent: Agent) {
    setBusy(agent.id);
    setError(null);
    try {
      await api.addAgent(channel, agent.id);
      await load();
      onChanged();
    } catch (failure) {
      setError(chatMessage(failure, strings.chatAgentAddFailed));
    } finally {
      setBusy(null);
    }
  }

  async function removeAgent(agent: Agent) {
    setBusy(agent.id);
    setError(null);
    try {
      await api.removeAgent(channel, agent.id);
      await load();
      onChanged();
    } catch (failure) {
      setError(chatMessage(failure, strings.chatAgentRemoveFailed));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-overlay p-4"
      role="dialog"
      aria-modal="true"
      aria-label={strings.chatWhoIsHere}
    >
      <div className="flex max-h-full min-h-0 w-full max-w-md flex-col overflow-hidden rounded-lg border border-subtle bg-surface shadow-lg">
        <header className="flex items-center justify-between gap-2 border-b border-subtle p-3">
          <h2 className="m-0 text-sm font-semibold text-primary">{strings.chatWhoIsHere}</h2>
          <IconButton
            onClick={onClose}
            label={strings.chatClose}
            icon={<X size={16} />}
            size="sm"
          />
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto p-3">
          <h3 className={sectionClass}>{strings.chatAgentsHere}</h3>
          {here.length === 0 ? (
            <p className="m-0 text-sm text-tertiary">{strings.chatNoAgentsHere}</p>
          ) : (
            <ul className={listClass}>
              {here.map((agent) => (
                <li key={agent.id} className={rowClass}>
                  <span className="flex size-6 shrink-0 items-center justify-center rounded-sm bg--tint text-accent">
                    <Sparkles size={13} />
                  </span>
                  <span className={whoClass}>
                    <span className="truncate text-sm text-primary">@{agent.handle}</span>
                    {/* What it has actually done, not what it is for. An
                        agent with a record reads as a colleague; one with
                        only a description reads as a setting. */}
                    <span className="truncate text-xs text-tertiary">
                      {agent.answers === 0
                        ? strings.chatAgentNothingYet
                        : strings.chatAgentRecord(agent.answers, agent.actions)}
                    </span>
                  </span>
                  <IconButton
                    onClick={() => void removeAgent(agent)}
                    disabled={busy === agent.id}
                    label={strings.chatAgentRemove(agent.handle)}
                    icon={<UserMinus size={15} />}
                    size="sm"
                  />
                </li>
              ))}
            </ul>
          )}

          {available.length > 0 && (
            <>
              <h3 className={sectionClass}>{strings.chatAgentsAvailable}</h3>
              <ul className={listClass}>
                {available.map((agent) => (
                  <li key={agent.id} className={rowClass}>
                    <span className="flex size-6 shrink-0 items-center justify-center rounded-sm bg--tint text-accent">
                      <Sparkles size={13} />
                    </span>
                    <span className={whoClass}>
                      <span className="truncate text-sm text-primary">@{agent.handle}</span>
                      {agent.description !== null && (
                        <span className="truncate text-xs text-tertiary">
                          {agent.description}
                        </span>
                      )}
                    </span>
                    <IconButton
                      onClick={() => void addAgent(agent)}
                      disabled={busy === agent.id}
                      label={strings.chatAgentAdd(agent.handle)}
                      icon={<UserPlus size={15} />}
                      size="sm"
                    />
                  </li>
                ))}
              </ul>
            </>
          )}

          <h3 className={sectionClass}>{strings.chatPeopleHere}</h3>
          <ul className={listClass}>
            {(detail?.members ?? []).map((member) => (
              <li key={member.user} className={rowClass}>
                <Avatar
                  name={member.email ?? member.user}
                  email={member.email ?? undefined}
                  size="sm"
                />
                <span className={whoClass}>
                  <span className="truncate text-sm text-primary">
                    {member.email ?? member.user}
                  </span>
                  {member.role === "owner" && (
                    <span className="truncate text-xs text-tertiary">{strings.chatOwner}</span>
                  )}
                </span>
              </li>
            ))}
          </ul>

          {error !== null && <p className="mt-3 text-sm text-accent" role="alert">{error}</p>}
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
