// Which parts of the workspace somebody actually uses.
//
// "Your favourites" defaulted to the first six modules in declaration order,
// which is a guess about a person made before they had done anything. This
// records what they open and ranks it, so the shortcut list is theirs rather
// than ours.
//
// **Frequency alone is wrong.** Somebody who lived in Billing during a month-end
// would keep seeing Billing in March, long after the work moved on. So a visit
// decays: recent use counts for more, and a module fades if it stops being
// opened. That is the difference between "what you have used most" and "what
// you are using now", and only the second is useful as a shortcut.
//
// **Local, not server-side.** Which apps you reach for is a fact about this
// device and this person's habits, and syncing it would mean a preference
// somebody never set following them onto a shared machine. If it should follow
// an account later, that is a deliberate decision with its own storage.

const KEY = "alo-app-usage";

/** How long a single visit takes to lose half its weight. */
const HALF_LIFE_DAYS = 14;

/** Never let one module's history grow without bound. */
const MAX_TRACKED = 40;

interface Visit {
  /** Accumulated, decayed score. */
  score: number;
  /** When `score` was last brought up to date. */
  at: number;
}

type Usage = Record<string, Visit>;

function read(): Usage {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (raw === null) return {};
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return {};
    return parsed as Usage;
  } catch {
    // A corrupt preference must never stop navigation from rendering.
    return {};
  }
}

function write(usage: Usage): void {
  try {
    // Keep the strongest entries only; a workspace has a bounded number of
    // modules but a corrupted or migrated store might not.
    const trimmed = Object.entries(usage)
      .sort(([, a], [, b]) => b.score - a.score)
      .slice(0, MAX_TRACKED);
    window.localStorage.setItem(
      KEY,
      JSON.stringify(Object.fromEntries(trimmed)),
    );
  } catch {
    // Private browsing, a full quota — none of which is worth an error.
  }
}

/** A visit's remaining weight after `ms` have passed. */
function decayed(score: number, since: number, now: number): number {
  const days = Math.max(0, now - since) / 86_400_000;
  return score * Math.pow(0.5, days / HALF_LIFE_DAYS);
}

/**
 * Record that somebody opened a module.
 *
 * Called on arrival rather than on click, so a module reached by any route —
 * a link in a message, a notification, the switcher — counts the same as one
 * reached from the rail.
 */
export function recordAppVisit(id: string): void {
  if (id === "" || id === "home") return;
  const now = Date.now();
  const usage = read();
  const previous = usage[id];
  usage[id] = {
    score:
      (previous === undefined ? 0 : decayed(previous.score, previous.at, now)) +
      1,
    at: now,
  };
  write(usage);
}

/**
 * The modules this person reaches for, strongest first.
 *
 * Scores are decayed at read time as well as at write time, so a list that has
 * not been written to for a month still ranks correctly.
 */
export function mostUsedApps(limit: number): string[] {
  const now = Date.now();
  return (
    Object.entries(read())
      .map(([id, visit]) => [id, decayed(visit.score, visit.at, now)] as const)
      // Below this a module has not been opened in months; keeping it would mean
      // a shortcut to something somebody has plainly stopped doing.
      .filter(([, score]) => score >= 0.05)
      .sort(([, a], [, b]) => b - a)
      .slice(0, limit)
      .map(([id]) => id)
  );
}
