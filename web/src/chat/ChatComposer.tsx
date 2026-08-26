import type { Dispatch, RefObject, SetStateAction } from "react";
import { Paperclip, Plus, Reply, Send, Smile, Sparkles, Users, X } from "lucide-react";

import { strings } from "../i18n";
import type { DriveNodeDto } from "../jmap/types";
import { ComposerShareMenu } from "./ComposerShareMenu";
import { EmojiPicker } from "./EmojiPicker";
import { FormattingToolbar } from "./FormattingToolbar";
import { channelLabel, personName } from "./presentation";
import type { Nameable } from "./presentation";
import type { ChannelSummary, Message } from "./types";

type ReplyContext = { message: Message; private: boolean };
type Props = {
  room: ChannelSummary;
  composerRef: RefObject<HTMLTextAreaElement | null>;
  menuRef: RefObject<HTMLDivElement | null>;
  draft: string;
  sending: boolean;
  reply: ReplyContext | null;
  staged: DriveNodeDto[];
  selected: boolean;
  menu: "share" | "emoji" | null;
  palette: string[];
  emojiQuery: string;
  suggestions: Nameable[];
  highlighted: number;
  onSubmit: () => void;
  onDraft: (value: string, caret: number, selected: boolean) => void;
  onSelect: (caret: number, selected: boolean) => void;
  onCaretToEnd: () => void;
  onHighlighted: Dispatch<SetStateAction<number>>;
  onComplete: (choice: Nameable) => void;
  onCancelReply: () => void;
  onUnstage: (id: string) => void;
  onMenu: Dispatch<SetStateAction<"share" | "emoji" | null>>;
  onPickFile: () => void;
  onAuthor: (kind: "code" | "equation") => void;
  onInsert: (text: string) => void;
  onEmojiQuery: (query: string) => void;
  onWrap: (before: string, after: string, sample: string) => void;
};

export function ChatComposer(props: Props) {
  const { room, composerRef, menuRef, draft, sending, reply, staged, selected, menu, palette, emojiQuery, suggestions, highlighted } = props;
  return <form className="relative mx-auto mb-4 flex w-full max-w-4xl flex-wrap items-end gap-1 rounded-xl border border-default bg-surface px-3 py-2 shadow-sm transition focus-within:border-accent focus-within:ring-2 focus-within:ring--tint max-sm:mx-2 max-sm:w-auto" onSubmit={(event) => { event.preventDefault(); props.onSubmit(); }}>
    {reply !== null && <div className="order-first flex w-full min-w-0 items-start gap-3 overflow-hidden border-b border-subtle px-2 pb-3 pt-1">
      <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-raised text-accent" aria-hidden="true"><Reply size={15} /></span>
      <span className="flex min-w-0 flex-1 flex-col gap-1"><strong>{reply.private ? strings.chatReplyingPrivately(personName(reply.message.authorEmail, reply.message.author)) : strings.chatReplyingHere}</strong><span className="line-clamp-2 break-words text-xs text-tertiary">{reply.message.body}</span></span>
      <button type="button" className="flex size-8 shrink-0 items-center justify-center rounded-sm border-0 bg-transparent text-tertiary hover:bg-raised hover:text-primary" onClick={props.onCancelReply} aria-label={strings.chatCancelReply}><X size={15} /></button>
    </div>}
    {selected && <FormattingToolbar wrap={props.onWrap} />}
    {staged.length > 0 && <ul className="order-first mb-2 flex w-full list-none flex-wrap gap-1 p-0">{staged.map((file) => <li key={file.id}><button type="button" className="inline-flex min-h-8 max-w-56 items-center gap-1 rounded-full border border-subtle bg-raised px-2 text-xs text-primary hover:border-default" onClick={() => props.onUnstage(file.id)} aria-label={strings.chatUnstage(file.name)}><Paperclip size={13} /><span className="truncate">{file.name}</span><X size={13} /></button></li>)}</ul>}
    <div className="flex shrink-0 items-center gap-2 pr-1" ref={menuRef}>
      <span className="relative inline-flex"><button type="button" className="flex size-9 items-center justify-center rounded-md border-0 bg-raised text-tertiary hover:text-primary focus-visible:outline-2 focus-visible:outline-accent" onClick={() => props.onMenu((at) => at === "share" ? null : "share")} aria-label={strings.chatShare} title={strings.chatShare} aria-expanded={menu === "share"}><Plus size={18} /></button>
        {menu === "share" && <ComposerShareMenu onFile={props.onPickFile} onCode={() => props.onAuthor("code")} onEquation={() => props.onAuthor("equation")} onMention={() => props.onInsert("@")} onAskAlo={() => props.onInsert("@alo ")} />}
      </span>
      <span className="relative inline-flex"><button type="button" className="flex size-9 items-center justify-center rounded-md border-0 bg-raised text-tertiary hover:text-primary focus-visible:outline-2 focus-visible:outline-accent" onClick={() => props.onMenu((at) => at === "emoji" ? null : "emoji")} aria-label={strings.chatInsertEmoji} title={strings.chatInsertEmoji} aria-expanded={menu === "emoji"}><Smile size={18} /></button>
        {menu === "emoji" && palette.length > 0 && <EmojiPicker query={emojiQuery} onQuery={props.onEmojiQuery} onChoose={(emoji) => { props.onMenu(null); props.onEmojiQuery(""); props.onInsert(emoji); }} />}
      </span>
    </div>
    {suggestions.length > 0 && <ul className="absolute bottom-full left-3 z-30 mb-2 max-h-64 w-80 list-none overflow-y-auto rounded-lg border border-subtle bg-surface p-1 shadow-lg" role="listbox">{suggestions.map((choice, index) => <li key={`${choice.agent}-${choice.handle}`}><button type="button" role="option" aria-selected={index === highlighted} className={`flex min-h-10 w-full items-center gap-2 rounded-md border-0 px-3 text-left text-sm transition-colors hover:bg-accent-soft hover:text-accent ${index === highlighted ? "bg-accent-soft text-accent" : "bg-transparent text-secondary"}`} onMouseDown={(event) => { event.preventDefault(); props.onComplete(choice); }}>{choice.agent ? <Sparkles size={13} className="shrink-0 text-accent" /> : <Users size={13} className="shrink-0 text-tertiary" />}<span className="shrink-0 font-semibold text-primary">@{choice.handle}</span><span className="truncate text-xs text-tertiary">{choice.label}</span></button></li>)}</ul>}
    <textarea ref={composerRef} rows={1} className="max-h-40 min-h-9 min-w-0 flex-1 resize-none overflow-y-auto border-0 bg-transparent px-2 py-2 font-ui text-sm leading-relaxed text-primary outline-none placeholder:text-tertiary" value={draft} onChange={(event) => props.onDraft(event.target.value, event.target.selectionStart ?? 0, event.target.selectionStart !== event.target.selectionEnd)} onSelect={(event) => props.onSelect(event.currentTarget.selectionStart ?? 0, event.currentTarget.selectionStart !== event.currentTarget.selectionEnd)} onKeyDown={(event) => {
      if (event.key === "Enter" && !event.shiftKey && suggestions.length === 0) { event.preventDefault(); props.onSubmit(); return; }
      if (suggestions.length === 0) return;
      if (event.key === "ArrowDown") { event.preventDefault(); props.onHighlighted((at) => (at + 1) % suggestions.length); }
      else if (event.key === "ArrowUp") { event.preventDefault(); props.onHighlighted((at) => (at - 1 + suggestions.length) % suggestions.length); }
      else if (event.key === "Enter" || event.key === "Tab") { event.preventDefault(); const choice = suggestions[highlighted]; if (choice !== undefined) props.onComplete(choice); }
      else if (event.key === "Escape") { event.preventDefault(); props.onCaretToEnd(); props.onHighlighted(0); }
    }} placeholder={strings.chatComposerPlaceholder(channelLabel(room))} aria-label={strings.chatComposerLabel} autoComplete="off" />
    <button type="submit" className="flex size-10 shrink-0 items-center justify-center rounded-md border-0 bg-accent text-on-accent hover:bg--hover disabled:bg-transparent disabled:text-tertiary" disabled={draft.trim() === "" || sending} aria-label={strings.chatSend} title={strings.chatSend}><Send size={17} /></button>
  </form>;
}
