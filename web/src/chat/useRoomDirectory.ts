import { useCallback, useEffect, useState } from "react";

import { useDialogs } from "../ds";
import { strings } from "../i18n";
import { chatMessage, useChatApi } from "./api";
import type { Channel, ChannelSummary, Message, Person } from "./types";

export function useRoomDirectory(onError: (message: string | null) => void) {
  const api = useChatApi();
  const dialogs = useDialogs();
  const [channels, setChannels] = useState<ChannelSummary[] | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [browsing, setBrowsing] = useState<Channel[] | null>(null);
  const [dmQuery, setDmQuery] = useState<string | null>(null);
  const [dmFound, setDmFound] = useState<Person[]>([]);
  const [finding, setFinding] = useState("");
  const [found, setFound] = useState<Message[] | null>(null);

  const loadChannels = useCallback(async () => {
    try {
      const rooms = await api.channels();
      setChannels(rooms);
      setOpenId((current) => current ?? rooms[0]?.id ?? null);
    } catch (failure) {
      onError(chatMessage(failure, strings.chatLoadFailed));
    }
  }, [api, onError]);

  useEffect(() => { void loadChannels(); }, [loadChannels]);

  async function find(query: string) {
    setFinding(query);
    if (query.trim() === "") { setFound(null); return; }
    try { setFound(await api.search(query)); }
    catch (failure) { onError(chatMessage(failure, strings.chatSearchFailed)); setFound([]); }
  }

  async function findPeople(query: string) {
    setDmQuery(query);
    if (query.trim().length < 2) { setDmFound([]); return; }
    try { setDmFound(await api.findPeople(query)); }
    catch (failure) { onError(chatMessage(failure, strings.chatPeopleFailed)); setDmFound([]); }
  }

  async function openDm(person: Person) {
    onError(null);
    try {
      const room = await api.createChannel({ kind: "dm", with: person.user });
      await loadChannels();
      setDmQuery(null);
      setDmFound([]);
      setOpenId(room.id);
    } catch (failure) { onError(chatMessage(failure, strings.chatDmFailed)); }
  }

  async function renameRoom(room: ChannelSummary) {
    const name = (await dialogs.prompt({ title: strings.chatRename, message: strings.chatRenamePrompt, defaultValue: room.name ?? "", confirmLabel: strings.chatRenameSave }))?.trim();
    if (name === undefined || name === null || name === "" || name === room.name) return;
    try { await api.renameChannel(room.id, { name }); await loadChannels(); }
    catch (failure) { onError(chatMessage(failure, strings.chatRenameFailed)); }
  }

  async function archiveRoom(room: ChannelSummary) {
    const sure = await dialogs.confirm({ title: strings.chatArchiveTitle(room.name ?? strings.chatDirectMessage), message: strings.chatArchiveWarning, confirmLabel: strings.chatArchiveConfirm });
    if (!sure) return;
    try { await api.archiveChannel(room.id); await loadChannels(); }
    catch (failure) { onError(chatMessage(failure, strings.chatArchiveFailed)); }
  }

  async function browse() {
    onError(null);
    try {
      const joinable = await api.joinable();
      const mine = (channels ?? []).filter((room) => room.kind === "channel" && room.visibility === "public" && room.archivedAt === null);
      setBrowsing([...joinable, ...mine].sort((a, b) => (a.name ?? "").localeCompare(b.name ?? "")));
    } catch (failure) { onError(chatMessage(failure, strings.chatBrowseFailed)); setBrowsing([]); }
  }

  async function joinRoom(channel: Channel) {
    onError(null);
    try { await api.join(channel.id); await loadChannels(); setBrowsing(null); setOpenId(channel.id); }
    catch (failure) { onError(chatMessage(failure, strings.chatJoinFailed)); }
  }

  async function createChannel() {
    const name = (await dialogs.prompt({ title: strings.chatNewChannel, message: strings.chatNewChannelPrompt, placeholder: strings.chatNewChannelPlaceholder, confirmLabel: strings.chatCreate }))?.trim();
    if (name === undefined || name === null || name === "") return;
    setCreating(true);
    onError(null);
    try { const room = await api.createChannel({ name }); await loadChannels(); setOpenId(room.id); }
    catch (failure) { onError(chatMessage(failure, strings.chatCreateFailed)); }
    finally { setCreating(false); }
  }

  return { channels, openId, setOpenId, creating, browsing, setBrowsing, dmQuery, setDmQuery, dmFound, setDmFound, finding, found, loadChannels, find, findPeople, openDm, renameRoom, archiveRoom, browse, joinRoom, createChannel };
}
