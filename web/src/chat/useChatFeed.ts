import { useCallback, useEffect, useRef, useState } from "react";

import { useJmapClient } from "../jmap/useJmapClient";
import { strings } from "../i18n";
import { chatMessage, useChatApi } from "./api";
import type { Nameable } from "./presentation";
import type { ChannelSummary, FeedMessage, Message, Proposal, Turn } from "./types";

const PAGE = 50;

export function useChatFeed(openId: string | null, channels: ChannelSummary[] | null, reloadRooms: () => Promise<void>, onError: (message: string | null) => void) {
  const api = useChatApi();
  const client = useJmapClient();
  const feedRef = useRef<HTMLDivElement | null>(null);
  const [messages, setMessages] = useState<FeedMessage[] | null>(null);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [palette, setPalette] = useState<string[]>([]);
  const [nameable, setNameable] = useState<Nameable[]>([]);
  const [readUpTo, setReadUpTo] = useState<number | null>(null);
  const [moreBehind, setMoreBehind] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);

  const loadTurns = useCallback(async (id: string) => {
    try { setTurns(await api.turns(id)); } catch { setTurns([]); }
  }, [api]);

  const loadMessages = useCallback(async (id: string) => {
    try {
      const page = await api.messages(id);
      setMessages([...page].reverse());
      setMoreBehind(page.length === PAGE);
      const newest = page[0]?.seq;
      if (newest !== undefined) await api.markRead(id, newest);
    } catch (failure) { onError(chatMessage(failure, strings.chatLoadFailed)); }
  }, [api, onError]);

  useEffect(() => { void api.reactionPalette().then(setPalette).catch(() => setPalette([])); }, [api]);

  useEffect(() => {
    if (openId === null) return;
    setMessages(null);
    setTurns([]);
    void loadMessages(openId);
    void loadTurns(openId);
  }, [loadMessages, loadTurns, openId]);

  // Capture the unread boundary once, when a room becomes available. Later
  // sidebar refreshes must not reopen the room and mark it read again.
  useEffect(() => {
    if (openId === null) return;
    const room = channels?.find((candidate) => candidate.id === openId);
    if (room !== undefined) setReadUpTo(room.lastReadSeq);
  }, [openId, channels]);

  useEffect(() => {
    if (openId === null) return;
    let live = true;
    void Promise.all([api.channel(openId).catch(() => null), api.channelAgents(openId).catch(() => [])]).then(([detail, agents]) => {
      if (!live) return;
      const people: Nameable[] = (detail?.members ?? []).map((member) => ({ handle: (member.email ?? member.user).split("@")[0]!.toLowerCase(), label: member.email ?? member.user, agent: false }));
      setNameable([...agents.map((agent) => ({ handle: agent.handle, label: agent.name, agent: true })), ...people]);
    });
    return () => { live = false; };
  }, [api, openId]);

  useEffect(() => {
    const controller = new AbortController();
    let live = true;
    void (async () => {
      while (live && !controller.signal.aborted) {
        try { await client.subscribeChat(() => { void reloadRooms(); if (openId !== null) { void loadMessages(openId); void loadTurns(openId); } }, controller.signal); }
        catch { await new Promise((resume) => setTimeout(resume, 3_000)); }
      }
    })();
    return () => { live = false; controller.abort(); };
  }, [client, loadMessages, loadTurns, openId, reloadRooms]);

  const newestSeq = messages?.[messages.length - 1]?.seq ?? null;
  useEffect(() => { const feed = feedRef.current; if (feed !== null) feed.scrollTop = feed.scrollHeight; }, [newestSeq, openId]);

  async function loadOlder() {
    const oldest = messages?.[0]?.seq;
    if (openId === null || oldest === undefined || loadingOlder) return;
    setLoadingOlder(true);
    const before = feedRef.current?.scrollHeight ?? 0;
    try {
      const page = await api.messages(openId, oldest);
      setMessages((held) => { const current = held ?? []; const known = new Set(current.map((message) => message.id)); return [...[...page].reverse().filter((message) => !known.has(message.id)), ...current]; });
      setMoreBehind(page.length === PAGE);
      requestAnimationFrame(() => { const feed = feedRef.current; if (feed !== null) feed.scrollTop += feed.scrollHeight - before; });
    } catch (failure) { onError(chatMessage(failure, strings.chatLoadFailed)); }
    finally { setLoadingOlder(false); }
  }

  async function editMessage(message: Message, body: string) { onError(null); try { await api.editMessage(message.id, body); } catch (failure) { onError(chatMessage(failure, strings.chatEditFailed)); } if (openId !== null) void loadMessages(openId); }
  async function withdrawMessage(message: Message) { onError(null); try { await api.withdrawMessage(message.id); } catch (failure) { onError(chatMessage(failure, strings.chatWithdrawFailed)); } if (openId !== null) void loadMessages(openId); }
  async function decide(proposal: Proposal, approve: boolean) { onError(null); try { await api.decideProposal(proposal.id, approve); } catch (failure) { onError(chatMessage(failure, strings.chatDecideFailed)); } if (openId !== null) void loadMessages(openId); }
  async function react(messageId: string, emoji: string) { try { const tally = await api.react(messageId, emoji); setMessages((list) => list?.map((message) => message.id === messageId ? { ...message, reactions: tally } : message) ?? null); } catch (failure) { onError(chatMessage(failure, strings.chatReactFailed)); } }

  return { feedRef, messages, setMessages, turns, palette, nameable, readUpTo, moreBehind, loadingOlder, loadMessages, loadTurns, loadOlder, editMessage, withdrawMessage, decide, react };
}
