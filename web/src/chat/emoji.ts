// The emoji a person can put in a message.
//
// This is deliberately NOT the reaction set. A reaction is a fixed vocabulary
// the server enforces (`REACTIONS`), because a reaction is a field displayed
// to the whole room and free choice there makes it a second, unmoderated
// message. Text is already free — an emoji typed into a sentence is just a
// character — so restricting the picker would only stop people finding what
// their keyboard can already produce.
//
// A curated list rather than a dependency. The full Unicode set is ~1900
// characters with names, keywords and skin-tone variants, and pulling a
// package for it would add a payload larger than this whole module to save
// writing a list. These are the ones people actually reach for at work,
// grouped the way pickers group them, with the words they would search by.

/** One group in the picker. */
export interface EmojiGroup {
  /** Shown as the group heading. */
  name: string;
  /** The tab glyph. */
  icon: string;
  /** `[glyph, searchable words]`. */
  items: [string, string][];
}

export const EMOJI: EmojiGroup[] = [
  {
    name: "People",
    icon: "😀",
    items: [
      ["😀", "grin smile happy"],
      ["😃", "smile happy joy"],
      ["😄", "smile happy laugh"],
      ["😁", "beam grin happy"],
      ["😅", "sweat laugh relief nervous"],
      ["😂", "laugh cry tears funny"],
      ["🙂", "slight smile"],
      ["😉", "wink"],
      ["😊", "blush smile warm"],
      ["😍", "love heart eyes"],
      ["😘", "kiss"],
      ["🤗", "hug"],
      ["🤔", "think hmm consider"],
      ["🤨", "eyebrow doubt sceptical"],
      ["😐", "neutral flat"],
      ["😴", "sleep tired"],
      ["😌", "relieved calm"],
      ["😔", "sad down"],
      ["😕", "confused"],
      ["🙁", "frown sad"],
      ["😢", "cry sad tear"],
      ["😭", "sob cry"],
      ["😤", "triumph determined"],
      ["😠", "angry cross"],
      ["😳", "flushed surprise embarrassed"],
      ["🥺", "pleading please"],
      ["😬", "grimace awkward"],
      ["🤯", "mind blown shocked"],
      ["😎", "cool sunglasses"],
      ["🤓", "nerd glasses"],
      ["🥳", "party celebrate"],
      ["😇", "innocent halo"],
      ["🤝", "handshake deal agree"],
      ["👋", "wave hello hi bye"],
      ["🙏", "please thanks pray"],
      ["👍", "yes good approve thumbs up"],
      ["👎", "no bad reject thumbs down"],
      ["👏", "clap applause well done"],
      ["🙌", "raise celebrate hooray"],
      ["💪", "strong muscle"],
      ["🤞", "fingers crossed hope luck"],
      ["👌", "ok perfect"],
      ["✌️", "peace victory"],
      ["🫡", "salute understood"],
      ["🤷", "shrug dunno"],
      ["👀", "eyes look watching"],
      ["🧠", "brain smart idea"],
    ],
  },
  {
    name: "Work",
    icon: "💼",
    items: [
      ["✅", "done tick check yes complete"],
      ["☑️", "checkbox done"],
      ["❌", "no wrong fail cross"],
      ["⚠️", "warning careful caution"],
      ["🚧", "wip blocked construction"],
      ["🔴", "red blocker urgent"],
      ["🟠", "orange at risk"],
      ["🟢", "green good on track"],
      ["🔵", "blue info"],
      ["📌", "pin important"],
      ["📎", "attach clip file"],
      ["📁", "folder files"],
      ["📄", "document page doc"],
      ["📊", "chart report data"],
      ["📈", "up growth increase"],
      ["📉", "down decrease loss"],
      ["🗓️", "calendar date schedule"],
      ["⏰", "alarm deadline time"],
      ["⏳", "waiting pending hourglass"],
      ["💡", "idea suggestion"],
      ["🔍", "search find look"],
      ["🔒", "lock private secure"],
      ["🔑", "key access"],
      ["🐛", "bug defect issue"],
      ["🚀", "ship launch release"],
      ["🎯", "target goal aim"],
      ["🏁", "finish done complete"],
      ["📝", "note write memo"],
      ["✏️", "edit pencil write"],
      ["🗑️", "delete bin trash remove"],
      ["♻️", "recycle refactor reuse"],
      ["🔗", "link url"],
      ["💬", "comment chat message"],
      ["📣", "announce shout news"],
      ["📮", "send post mail"],
      ["💰", "money cost budget revenue"],
      ["🧾", "invoice receipt bill"],
      ["⚖️", "legal balance compliance"],
      ["🛠️", "tools fix build"],
      ["⚙️", "settings config gear"],
      ["🧪", "test experiment"],
      ["🧹", "clean tidy sweep"],
    ],
  },
  {
    name: "Things",
    icon: "🎉",
    items: [
      ["🎉", "party celebrate hooray"],
      ["🎊", "confetti celebrate"],
      ["🥂", "cheers toast"],
      ["☕", "coffee break"],
      ["🍕", "pizza food lunch"],
      ["🍰", "cake birthday"],
      ["🔥", "fire hot great"],
      ["⭐", "star favourite"],
      ["✨", "sparkles new magic ai"],
      ["❤️", "love heart red"],
      ["💚", "green heart"],
      ["💙", "blue heart"],
      ["🖤", "black heart"],
      ["💯", "hundred perfect agree"],
      ["👑", "crown best win"],
      ["🏆", "trophy win award"],
      ["🎁", "gift present"],
      ["🌍", "world earth global europe"],
      ["🌱", "seed grow new"],
      ["☀️", "sun sunny"],
      ["🌧️", "rain"],
      ["❄️", "snow cold freeze"],
      ["🌙", "moon night"],
      ["🚗", "car travel drive"],
      ["✈️", "plane travel flight"],
      ["🏠", "home house remote"],
      ["🏢", "office building work"],
      ["💻", "laptop computer code"],
      ["📱", "phone mobile"],
      ["🖨️", "printer print"],
      ["🔋", "battery power energy"],
      ["📡", "signal network live"],
    ],
  },
];

/** Every glyph, flattened — for a search across all groups. */
export function searchEmoji(query: string): string[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [];
  const hits: string[] = [];
  for (const group of EMOJI) {
    for (const [glyph, words] of group.items) {
      if (words.includes(q) || words.split(" ").some((w) => w.startsWith(q))) {
        hits.push(glyph);
      }
    }
  }
  return hits;
}
