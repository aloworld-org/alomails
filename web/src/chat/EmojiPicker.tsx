import { strings } from "../i18n";
import { EMOJI, searchEmoji } from "./emoji";

export function EmojiPicker({
  query,
  onQuery,
  onChoose,
}: {
  query: string;
  onQuery: (query: string) => void;
  onChoose: (emoji: string) => void;
}) {
  const hits = query.trim() === "" ? null : searchEmoji(query);
  const optionClass = "flex size-9 items-center justify-center rounded-sm border-0 bg-transparent text-lg hover:bg--tint focus-visible:bg--tint focus-visible:outline-2 focus-visible:outline-accent";
  return (
    <section className="absolute bottom-full left-0 z-30 mb-2 flex max-h-80 w-80 flex-col overflow-hidden rounded-lg border border-subtle bg-surface shadow-lg max-sm:w-64">
      <div className="border-b border-subtle p-2">
        <input
          className="min-h-10 w-full rounded-md border border-subtle bg-app px-3 text-sm text-primary outline-none placeholder:text-tertiary focus:border-accent focus:ring-2 focus:ring--tint"
          value={query}
          onChange={(event) => onQuery(event.target.value)}
          placeholder={strings.chatEmojiSearch}
          aria-label={strings.chatEmojiSearch}
          autoComplete="off"
          autoFocus
        />
      </div>
      <div className="min-h-0 overflow-y-auto p-2">
        {hits !== null ? (
          hits.length === 0 ? (
            <p className="m-0 px-2 py-4 text-center text-sm text-tertiary">{strings.chatEmojiNone}</p>
          ) : (
            <div className="grid grid-cols-7 gap-1 max-sm:grid-cols-6">
              {hits.map((emoji) => <button key={emoji} type="button" className={optionClass} onClick={() => onChoose(emoji)}>{emoji}</button>)}
            </div>
          )
        ) : (
          EMOJI.map((group) => (
            <section key={group.name} className="mb-3 last:mb-0">
              <h4 className="mb-1 px-1 text-xs font-semibold uppercase tracking-wide text-tertiary">{group.name}</h4>
              <div className="grid grid-cols-7 gap-1 max-sm:grid-cols-6">
                {group.items.map(([emoji]) => <button key={emoji} type="button" className={optionClass} onClick={() => onChoose(emoji)}>{emoji}</button>)}
              </div>
            </section>
          ))
        )}
      </div>
    </section>
  );
}
