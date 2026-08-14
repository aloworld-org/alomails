import { useState, type ReactNode } from "react";
import { Archive, ChevronDown, ChevronRight, Hash, Loader2, Lock, MoreHorizontal, Pencil, Search, SlidersHorizontal, Users, X } from "lucide-react";

import { Avatar, IconButton } from "../ds";
import { strings } from "../i18n";
import { channelLabel, directMessageName, personName } from "./presentation";
import type { Channel, ChannelSummary, Message, Person } from "./types";

interface ChatSidebarProps {
  channels: ChannelSummary[] | null;
  openId: string | null;
  creating: boolean;
  finding: string;
  found: Message[] | null;
  dmQuery: string | null;
  dmFound: Person[];
  browsing: Channel[] | null;
  rowMenu: string | null;
  onCreateChannel: () => void;
  onStartDm: () => void;
  onBrowse: () => void;
  onFind: (query: string) => void;
  onFindPeople: (query: string) => void;
  onCloseDm: () => void;
  onOpenDm: (person: Person) => void;
  onCloseBrowse: () => void;
  onJoinRoom: (room: Channel) => void;
  onOpen: (id: string) => void;
  onRowMenu: (id: string | null) => void;
  onRename: (room: ChannelSummary) => void;
  onArchive: (room: ChannelSummary) => void;
}

const iconClass = "shrink-0 text-tertiary";
const listClass = "m-0 flex list-none flex-col gap-0.5 p-0";
const rowClass = "flex min-h-[3.25rem] w-full items-center gap-4 rounded-2xl border border-transparent bg-transparent px-4 text-left text-base text-primary transition-colors duration-150 hover:bg-raised focus-visible:outline-2 focus-visible:outline-accent";

export function ChatSidebar(props: ChatSidebarProps) {
  const rooms = props.channels ?? [];
  const [filter, setFilter] = useState<"all" | "unread" | "threads" | "mentions">("all");
  const [composeOpen, setComposeOpen] = useState(false);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const visibleRooms = rooms.filter((room) => filter === "all"
    || (filter === "unread" && room.unread > 0)
    || (filter === "mentions" && room.mentions > 0)
    || (filter === "threads" && room.lastSeq !== null));
  const sections = [
    { key: "channels", label: strings.chatSectionChannels, rooms: visibleRooms.filter((room) => room.kind === "channel" && room.archivedAt === null) },
    { key: "direct", label: strings.chatSectionDirect, rooms: visibleRooms.filter((room) => room.kind === "dm" && room.archivedAt === null) },
    { key: "archived", label: strings.chatSectionArchived, rooms: visibleRooms.filter((room) => room.archivedAt !== null) },
  ].filter((section) => section.rooms.length > 0);

  return (
    <aside className="flex min-h-0 w-[24rem] shrink-0 flex-col border-r border-subtle bg-surface max-xl:w-80 max-md:w-full">
      <header className="flex min-h-16 items-center justify-between px-5">
        <h2 className="m-0 flex items-center gap-2 text-lg font-bold tracking-[-0.02em] text-primary">{strings.moduleChat}<ChevronDown size={15} className="text-tertiary" /></h2>
        <span className="relative rounded-lg border border-subtle bg-surface"><IconButton size="sm" label={strings.chatCompose} icon={<Pencil size={16} />} onClick={() => setComposeOpen((open) => !open)} active={composeOpen} />{composeOpen && <span className="absolute right-0 top-full z-30 mt-2 flex min-w-44 flex-col rounded-xl border border-subtle bg-surface p-1.5 shadow-md"><button type="button" className="flex min-h-10 items-center gap-2 rounded-lg px-3 text-sm text-primary hover:bg-raised" onClick={() => { setComposeOpen(false); props.onCreateChannel(); }}><Hash size={15} />{strings.chatNewChannel}</button><button type="button" className="flex min-h-10 items-center gap-2 rounded-lg px-3 text-sm text-primary hover:bg-raised" onClick={() => { setComposeOpen(false); props.onStartDm(); }}><Users size={15} />{strings.chatNewDm}</button></span>}</span>
      </header>

      <label className="mx-5 mb-3 flex min-h-10 items-center gap-2.5 rounded-lg border border-subtle bg-surface px-3 transition focus-within:border-accent focus-within:ring-2 focus-within:ring--tint">
        <Search size={15} className={iconClass} />
        <input className="min-w-0 flex-1 border-0 bg-transparent text-sm text-primary outline-none placeholder:text-tertiary" value={props.finding} onChange={(event) => props.onFind(event.target.value)} placeholder={strings.chatSearchPlaceholder} aria-label={strings.chatSearchPlaceholder} autoComplete="off" />
        {props.finding !== "" && <IconButton size="sm" label={strings.chatSearchClear} icon={<X size={14} />} onClick={() => props.onFind("")} />}
      </label>

      <div className="mb-3 flex min-h-9 items-center gap-3 px-5">
        {(["all", "unread", "threads", "mentions"] as const).map((value) => { const active = filter === value; return <button key={value} type="button" aria-pressed={active} style={{ minHeight: 32, padding: "0 12px", borderRadius: 9, border: `1px solid ${active ? "var(--accent)" : "var(--border-subtle)"}`, background: active ? "var(--accent)" : "var(--bg-surface)", color: active ? "var(--text-on-accent)" : "var(--text-primary)", fontSize: 12, fontWeight: 600 }} className="transition-colors" onClick={() => setFilter(value)}>{value === "all" ? strings.chatFilterAll : value === "unread" ? strings.chatFilterUnread : value === "threads" ? strings.chatFilterThreads : strings.chatFilterMentions}</button>; })}
        <button type="button" style={{ border: "1px solid var(--border-subtle)", background: "var(--bg-surface)" }} className="ml-auto flex size-8 items-center justify-center rounded-lg text-secondary transition-colors hover:text-primary" onClick={props.onBrowse} title={strings.chatBrowse}><SlidersHorizontal size={15} /><span className="sr-only">{strings.chatBrowse}</span></button>
      </div>
      <div className="mb-4 grid grid-cols-2 gap-2 px-5">
        <button type="button" style={{ minHeight: 42, borderRadius: 9, border: "1px solid transparent", background: "var(--accent-soft)", color: "var(--accent)" }} className="flex items-center justify-center gap-2 px-3 text-xs font-semibold transition-colors" onClick={props.onCreateChannel} disabled={props.creating}><Hash size={14} />{strings.chatNewChannel}</button>
        <button type="button" style={{ minHeight: 42, borderRadius: 9, border: "1px solid var(--border-subtle)", background: "var(--bg-raised)", color: "var(--text-primary)" }} className="flex items-center justify-center gap-2 px-3 text-xs font-semibold transition-colors" onClick={props.onStartDm}><Users size={14} />{strings.chatNewDm}</button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-5">
        {props.dmQuery !== null ? (
          <SidebarPanel title={strings.chatNewDm} onClose={props.onCloseDm}>
            <label className="mb-2 flex min-h-10 items-center gap-2 rounded-md border border-subtle bg-app px-3 focus-within:border-accent">
              <Users size={15} className={iconClass} />
              <input className="min-w-0 flex-1 border-0 bg-transparent text-sm outline-none" value={props.dmQuery} onChange={(event) => props.onFindPeople(event.target.value)} placeholder={strings.chatFindPerson} aria-label={strings.chatFindPerson} autoComplete="off" autoFocus />
            </label>
            {props.dmFound.length === 0 ? (
              <Note>{props.dmQuery.trim().length < 2 ? strings.chatFindPersonHint : strings.chatNobodyFound}</Note>
            ) : (
              <ul className={listClass}>{props.dmFound.map((person) => <li key={person.user}><button type="button" className={rowClass} onClick={() => props.onOpenDm(person)}><Avatar name={person.email} email={person.email} size="sm" /><span className="truncate">{person.email}</span></button></li>)}</ul>
            )}
          </SidebarPanel>
        ) : props.browsing !== null ? (
          <SidebarPanel title={strings.chatBrowse} onClose={props.onCloseBrowse}>
            {props.browsing.length === 0 ? <Note>{strings.chatNothingToJoin}</Note> : (
              <ul className={listClass}>{props.browsing.map((room) => {
                const joined = rooms.some((candidate) => candidate.id === room.id);
                return <li key={room.id}><button type="button" className={rowClass} onClick={() => joined ? props.onOpen(room.id) : props.onJoinRoom(room)}><Hash size={15} className={iconClass} /><span className="min-w-0 flex-1 truncate">{room.name}</span><span className="text-xs text-tertiary">{joined ? strings.chatJoined : strings.chatJoin}</span></button></li>;
              })}</ul>
            )}
          </SidebarPanel>
        ) : props.found !== null ? (
          props.found.length === 0 ? <Note>{strings.chatSearchNothing}</Note> : (
            <ul className={listClass}>{props.found.map((hit) => <li key={hit.id}><button type="button" className={`${rowClass} flex-col items-start gap-1 py-2`} onClick={() => { props.onOpen(hit.channel); props.onFind(""); }}><strong className="text-xs">{hit.authorKind === "agent" ? (hit.authorEmail ?? hit.author) : personName(hit.authorEmail, hit.author)}</strong><span className="line-clamp-2 text-xs text-secondary">{hit.body}</span></button></li>)}</ul>
          )
        ) : props.channels === null ? (
          <Note><Loader2 className="inline animate-spin" size={14} /> {strings.chatLoading}</Note>
        ) : props.channels.length === 0 ? (
          <div className="px-3 py-8 text-center"><p className="m-0 font-semibold text-primary">{strings.chatNoChannelsLead}</p><p className="mt-2 text-sm text-tertiary">{strings.chatNoChannelsHint}</p></div>
        ) : (
          <div>{sections.map((section) => { const isCollapsed = collapsed.has(section.key); return <section key={section.key} className="mb-7"><div className="mb-2 flex min-h-10 items-center justify-between px-2"><button type="button" className="flex min-w-0 items-center gap-2 rounded-lg px-1 py-1 text-base font-bold text-primary hover:bg-raised" onClick={() => setCollapsed((current) => { const next = new Set(current); if (next.has(section.key)) next.delete(section.key); else next.add(section.key); return next; })}>{isCollapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}<span>{section.label}</span></button>{section.label !== strings.chatSectionArchived && <button type="button" className="flex size-8 items-center justify-center rounded-lg text-xl font-normal text-primary transition-colors hover:bg-raised" aria-label={section.label === strings.chatSectionChannels ? strings.chatNewChannel : strings.chatNewDm} onClick={section.label === strings.chatSectionChannels ? props.onCreateChannel : props.onStartDm}>+</button>}</div>{!isCollapsed && <ul className={listClass}>{section.rooms.map((room) => <RoomRow key={room.id} room={room} open={room.id === props.openId} menuOpen={props.rowMenu === room.id} onOpen={() => props.onOpen(room.id)} onMenu={() => props.onRowMenu(props.rowMenu === room.id ? null : room.id)} onRename={() => props.onRename(room)} onArchive={() => props.onArchive(room)} />)}</ul>}</section>; })}</div>
        )}
      </div>
    </aside>
  );
}

function SidebarPanel({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  return <section className="rounded-lg border border-subtle bg-app p-2"><header className="mb-2 flex items-center justify-between px-1"><strong className="text-sm text-primary">{title}</strong><IconButton size="sm" label={strings.chatClose} icon={<X size={14} />} onClick={onClose} /></header>{children}</section>;
}

function Note({ children }: { children: ReactNode }) {
  return <p className="m-0 px-3 py-4 text-sm text-tertiary">{children}</p>;
}

function sidebarTime(value: string | null): string | null {
  if (value === null) return null;
  const at = new Date(value);
  if (Number.isNaN(at.getTime())) return null;
  const now = new Date();
  if (at.toDateString() === now.toDateString()) return at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (at.toDateString() === yesterday.toDateString()) return strings.chatYesterday;
  return at.toLocaleDateString([], { month: "short", day: "numeric" });
}

function RoomRow({ room, open, menuOpen, onOpen, onMenu, onRename, onArchive }: { room: ChannelSummary; open: boolean; menuOpen: boolean; onOpen: () => void; onMenu: () => void; onRename: () => void; onArchive: () => void }) {
  const roomIconClass = open ? "shrink-0 text-accent" : iconClass;
  const label = room.kind === "dm" ? directMessageName(room) : channelLabel(room);
  const at = sidebarTime(room.lastAt);
  const rowStyle = {
    ...(open ? { background: "linear-gradient(90deg, #FFF1EC 0%, #FCE9E3 100%)", borderColor: "transparent" } : {}),
    paddingLeft: room.kind === "channel" ? "1.5rem" : "0.5rem",
    paddingRight: "1rem",
  };

  return <li className="group relative flex items-center"><button type="button" aria-current={open ? "page" : undefined} style={rowStyle} className={`${rowClass} ${room.kind === "dm" ? "py-2" : ""} ${open ? "font-medium" : ""} ${room.archivedAt !== null ? "opacity-60" : ""}`} onClick={onOpen}>{room.kind === "dm" ? <Avatar name={label} email={room.counterpart ?? undefined} size="sm" /> : room.visibility === "private" ? <Lock size={18} className={roomIconClass} /> : <Hash size={18} className={roomIconClass} />}<span className="min-w-0 flex-1">{room.kind === "dm" ? <span className="grid min-w-0 grid-cols-[1fr_auto] gap-x-2"><span className="truncate font-semibold">{label}</span>{at !== null && <span className="text-[0.68rem] font-normal text-tertiary">{at}</span>}<span className="col-span-2 truncate text-xs font-normal text-tertiary">{room.lastBody ?? room.counterpart}</span></span> : <span className="block truncate">{label}</span>}</span>{room.archivedAt !== null && <Archive size={13} className={iconClass} aria-label={strings.chatArchived} />}{room.mentions > 0 ? <span className="flex size-6 items-center justify-center rounded-full bg-accent text-xs font-bold text-on-accent">{room.mentions}</span> : room.unread > 0 ? <span className="flex size-6 items-center justify-center rounded-full bg-accent text-xs font-bold text-on-accent">{room.unread}</span> : null}</button>{room.kind === "channel" && room.archivedAt === null && room.unread === 0 && room.mentions === 0 && <span className="absolute right-2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"><IconButton size="sm" label={strings.chatChannelActions(channelLabel(room))} icon={<MoreHorizontal size={16} />} onClick={onMenu} active={menuOpen} />{menuOpen && <span className="absolute right-0 top-full z-20 mt-1 flex min-w-36 flex-col rounded-md border border-subtle bg-surface p-1 shadow-md" role="menu"><button type="button" role="menuitem" className="flex min-h-10 items-center gap-2 rounded-sm px-3 text-sm hover:bg-raised" onClick={onRename}><Pencil size={14} />{strings.chatRename}</button><button type="button" role="menuitem" className="flex min-h-10 items-center gap-2 rounded-sm px-3 text-sm hover:bg-raised" onClick={onArchive}><Archive size={14} />{strings.chatArchiveAction}</button></span>}</span>}</li>;
}
