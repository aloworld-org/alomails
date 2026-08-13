import type { ReactNode } from "react";
import { Archive, ChevronDown, Hash, Loader2, Lock, MessageSquarePlus, MoreHorizontal, Pencil, Search, SlidersHorizontal, Users, X } from "lucide-react";

import { Avatar, Badge, Button, IconButton } from "../ds";
import { strings } from "../i18n";
import { channelLabel, personName } from "./presentation";
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
const listClass = "m-0 flex list-none flex-col gap-1.5 p-0";
const rowClass = "flex min-h-12 w-full items-center gap-3 rounded-xl border border-transparent bg-transparent px-3 text-left text-sm text-primary transition-colors duration-150 hover:border-subtle hover:bg-raised focus-visible:outline-2 focus-visible:outline-accent";

export function ChatSidebar(props: ChatSidebarProps) {
  const rooms = props.channels ?? [];
  const sections = [
    { label: strings.chatSectionChannels, rooms: rooms.filter((room) => room.kind === "channel" && room.archivedAt === null) },
    { label: strings.chatSectionDirect, rooms: rooms.filter((room) => room.kind === "dm" && room.archivedAt === null) },
    { label: strings.chatSectionArchived, rooms: rooms.filter((room) => room.archivedAt !== null) },
  ].filter((section) => section.rooms.length > 0);

  return (
    <aside className="flex min-h-0 w-[25rem] shrink-0 flex-col border-r border-subtle bg-surface max-xl:w-96 max-md:w-full">
      <header className="flex min-h-20 items-center justify-between px-6">
        <h2 className="m-0 flex items-center gap-2 text-xl font-bold tracking-[-0.02em] text-primary">{strings.moduleChat}<ChevronDown size={16} className="text-tertiary" /></h2>
        <span className="rounded-xl border border-subtle bg-surface shadow-sm"><IconButton size="md" label={strings.chatNewDm} icon={<Pencil size={18} />} onClick={props.onStartDm} /></span>
      </header>

      <label className="mx-6 mb-4 flex min-h-12 items-center gap-3 rounded-xl border border-subtle bg-app px-4 transition focus-within:border-accent focus-within:bg-surface focus-within:ring-2 focus-within:ring--tint">
        <Search size={17} className={iconClass} />
        <input className="min-w-0 flex-1 border-0 bg-transparent text-[0.95rem] text-primary outline-none placeholder:text-tertiary" value={props.finding} onChange={(event) => props.onFind(event.target.value)} placeholder={strings.chatSearchPlaceholder} aria-label={strings.chatSearchPlaceholder} autoComplete="off" />
        {props.finding !== "" && <IconButton size="sm" label={strings.chatSearchClear} icon={<X size={14} />} onClick={() => props.onFind("")} />}
      </label>

      <div className="mb-5 grid grid-cols-[auto_1fr_1fr_auto] items-center gap-2 px-6">
        <span className="inline-flex min-h-9 items-center rounded-lg bg-accent-soft px-3 text-xs font-semibold text-accent">All</span>
        <Button variant="secondary" size="sm" icon={<MessageSquarePlus size={15} />} onClick={props.onCreateChannel} disabled={props.creating}>{strings.chatNewChannel}</Button>
        <Button variant="secondary" size="sm" icon={<Users size={15} />} onClick={props.onStartDm}>{strings.chatNewDm}</Button>
        <button type="button" className="flex size-9 items-center justify-center rounded-lg border border-subtle bg-surface text-secondary transition-colors hover:border-accent hover:text-accent" onClick={props.onBrowse} title={strings.chatBrowse}><SlidersHorizontal size={16} /><span className="sr-only">{strings.chatBrowse}</span></button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-5">
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
          <div>{sections.map((section) => <section key={section.label} className="mb-6"><div className="mb-2 flex min-h-8 items-center justify-between px-2"><h3 className="m-0 text-[0.78rem] font-bold uppercase tracking-[0.08em] text-secondary">{section.label}</h3>{section.label !== strings.chatSectionArchived && <button type="button" className="flex size-7 items-center justify-center rounded-lg text-secondary transition-colors hover:bg-raised hover:text-primary" aria-label={section.label === strings.chatSectionChannels ? strings.chatNewChannel : strings.chatNewDm} onClick={section.label === strings.chatSectionChannels ? props.onCreateChannel : props.onStartDm}>+</button>}</div><ul className={listClass}>{section.rooms.map((room) => <RoomRow key={room.id} room={room} open={room.id === props.openId} menuOpen={props.rowMenu === room.id} onOpen={() => props.onOpen(room.id)} onMenu={() => props.onRowMenu(props.rowMenu === room.id ? null : room.id)} onRename={() => props.onRename(room)} onArchive={() => props.onArchive(room)} />)}</ul></section>)}</div>
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

function RoomRow({ room, open, menuOpen, onOpen, onMenu, onRename, onArchive }: { room: ChannelSummary; open: boolean; menuOpen: boolean; onOpen: () => void; onMenu: () => void; onRename: () => void; onArchive: () => void }) {
  const roomIconClass = open ? "shrink-0 text-accent" : iconClass;
  return <li className="group relative flex items-center"><button type="button" aria-current={open ? "page" : undefined} style={open ? { backgroundColor: "var(--accent-soft)", borderColor: "color-mix(in srgb, var(--accent) 34%, transparent)" } : undefined} className={`${rowClass} pr-10 ${open ? "font-semibold shadow-sm" : ""} ${room.archivedAt !== null ? "opacity-60" : ""}`} onClick={onOpen}>{room.kind === "dm" ? <Avatar name={channelLabel(room)} email={room.counterpart ?? undefined} size="sm" /> : room.visibility === "private" ? <Lock size={15} className={roomIconClass} /> : <Hash size={15} className={roomIconClass} />}<span className="min-w-0 flex-1 truncate">{channelLabel(room)}</span>{room.archivedAt !== null && <Archive size={13} className={iconClass} aria-label={strings.chatArchived} />}{room.mentions > 0 ? <Badge tone="accent">@{room.mentions}</Badge> : room.unread > 0 ? <Badge>{room.unread}</Badge> : null}</button>{room.kind === "channel" && room.archivedAt === null && <span className="absolute right-1 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"><IconButton size="sm" label={strings.chatChannelActions(channelLabel(room))} icon={<MoreHorizontal size={16} />} onClick={onMenu} active={menuOpen} />{menuOpen && <span className="absolute right-0 top-full z-20 mt-1 flex min-w-36 flex-col rounded-md border border-subtle bg-surface p-1 shadow-md" role="menu"><button type="button" role="menuitem" className="flex min-h-10 items-center gap-2 rounded-sm px-3 text-sm hover:bg-raised" onClick={onRename}><Pencil size={14} />{strings.chatRename}</button><button type="button" role="menuitem" className="flex min-h-10 items-center gap-2 rounded-sm px-3 text-sm hover:bg-raised" onClick={onArchive}><Archive size={14} />{strings.chatArchiveAction}</button></span>}</span>}</li>;
}
