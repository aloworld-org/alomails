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

import { Avatar, Button } from "../ds";
import { strings } from "../i18n";
import { chatMessage, useChatApi } from "./api";
import type { Agent, ChannelDetail } from "./types";
import styles from "./RoomPeople.module.css";

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
      className={styles.backdrop}
      role="dialog"
      aria-modal="true"
      aria-label={strings.chatWhoIsHere}
    >
      <div className={styles.panel}>
        <header className={styles.header}>
          <h2 className={styles.title}>{strings.chatWhoIsHere}</h2>
          <button
            type="button"
            className={styles.close}
            onClick={onClose}
            aria-label={strings.chatClose}
          >
            <X size={16} />
          </button>
        </header>

        <div className={styles.body}>
          <h3 className={styles.section}>{strings.chatAgentsHere}</h3>
          {here.length === 0 ? (
            <p className={styles.note}>{strings.chatNoAgentsHere}</p>
          ) : (
            <ul className={styles.list}>
              {here.map((agent) => (
                <li key={agent.id} className={styles.row}>
                  <span className={styles.mark}>
                    <Sparkles size={13} />
                  </span>
                  <span className={styles.who}>
                    <span className={styles.name}>@{agent.handle}</span>
                    {/* What it has actually done, not what it is for. An
                        agent with a record reads as a colleague; one with
                        only a description reads as a setting. */}
                    <span className={styles.detail}>
                      {agent.answers === 0
                        ? strings.chatAgentNothingYet
                        : strings.chatAgentRecord(agent.answers, agent.actions)}
                    </span>
                  </span>
                  <button
                    type="button"
                    className={styles.action}
                    onClick={() => void removeAgent(agent)}
                    disabled={busy === agent.id}
                    aria-label={strings.chatAgentRemove(agent.handle)}
                    title={strings.chatAgentRemove(agent.handle)}
                  >
                    <UserMinus size={15} />
                  </button>
                </li>
              ))}
            </ul>
          )}

          {available.length > 0 && (
            <>
              <h3 className={styles.section}>{strings.chatAgentsAvailable}</h3>
              <ul className={styles.list}>
                {available.map((agent) => (
                  <li key={agent.id} className={styles.row}>
                    <span className={styles.mark}>
                      <Sparkles size={13} />
                    </span>
                    <span className={styles.who}>
                      <span className={styles.name}>@{agent.handle}</span>
                      {agent.description !== null && (
                        <span className={styles.detail}>
                          {agent.description}
                        </span>
                      )}
                    </span>
                    <button
                      type="button"
                      className={styles.action}
                      onClick={() => void addAgent(agent)}
                      disabled={busy === agent.id}
                      aria-label={strings.chatAgentAdd(agent.handle)}
                      title={strings.chatAgentAdd(agent.handle)}
                    >
                      <UserPlus size={15} />
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}

          <h3 className={styles.section}>{strings.chatPeopleHere}</h3>
          <ul className={styles.list}>
            {(detail?.members ?? []).map((member) => (
              <li key={member.user} className={styles.row}>
                <Avatar
                  name={member.email ?? member.user}
                  email={member.email ?? undefined}
                  size="sm"
                />
                <span className={styles.who}>
                  <span className={styles.name}>
                    {member.email ?? member.user}
                  </span>
                  {member.role === "owner" && (
                    <span className={styles.detail}>{strings.chatOwner}</span>
                  )}
                </span>
              </li>
            ))}
          </ul>

          {error !== null && <p className={styles.error}>{error}</p>}
        </div>

        <footer className={styles.footer}>
          <Button variant="ghost" onClick={onClose}>
            {strings.chatClose}
          </Button>
        </footer>
      </div>
    </div>
  );
}
