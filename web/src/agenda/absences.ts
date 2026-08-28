// The absence layer of the calendar (B7.03): who is not here, drawn *behind*
// the month and week views, exactly as `docs/design/hr.md` ("The absence
// layer, and why it is not a calendar") prescribes — a derived read, never
// events. An absence here is a name and a day, nothing more: the feed behind
// it does not even select the kind of leave or the note, so the calendar
// cannot leak what was never loaded.
//
// Agenda is shared with the standalone mail product, which has no HR — so this
// file does not know where absences come from. The product surface that has
// them (the workspace) provides a source through `AbsenceLayerContext`, the
// same reasoning `ProductRailWidget` gives for rail widgets: the shared piece
// declares the seam, the product that owns the module fills it. With no
// provider the layer is simply empty, which is the standalone mail product's
// ordinary state.
import { createContext, useContext, useEffect, useMemo, useState } from "react";

/** One person who is away — everything the layer discloses about them. */
export interface AbsentColleague {
  /** A stable id, so a list of names keeps distinct keys for two Annas. */
  id: string;
  /** Their name as the directory shows it. */
  name: string;
}

/** One day somebody is away. Days with nobody away are not in the answer. */
export interface AbsenceDay {
  /** `YYYY-MM-DD`, the calendar's local day. */
  day: string;
  people: AbsentColleague[];
}

/** Who is away between two days, both ends inclusive, `YYYY-MM-DD`. */
export type AbsenceSource = (from: string, to: string) => Promise<AbsenceDay[]>;

/** The product's absence feed, or `null` where the product has none. */
export const AbsenceLayerContext = createContext<AbsenceSource | null>(null);

/** The calendar works in local time and converts at the edges — this is that
 *  edge for a day: the `YYYY-MM-DD` the user's clock would write on it. */
export function localDayKey(d: Date): string {
  const month = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${month}-${day}`;
}

/**
 * Who is away on each day of `[from, to]`, as day-key → people.
 *
 * Empty when the product has no absence layer, and empty again when the feed
 * fails: the layer is background — the authoritative screen with the error
 * banner is People → Who's away, and a calendar that refused to draw events
 * because HR was down would be the tail wagging the dog.
 */
export function useAbsenceLayer(
  from: Date,
  to: Date,
): ReadonlyMap<string, AbsentColleague[]> {
  const source = useContext(AbsenceLayerContext);
  const [days, setDays] = useState<AbsenceDay[]>([]);
  const fromKey = localDayKey(from);
  const toKey = localDayKey(to);

  useEffect(() => {
    if (source === null) {
      setDays([]);
      return;
    }
    let live = true;
    source(fromKey, toKey)
      .then((answer) => {
        if (live) setDays(answer);
      })
      .catch(() => {
        if (live) setDays([]);
      });
    return () => {
      live = false;
    };
  }, [source, fromKey, toKey]);

  return useMemo(() => {
    const byDay = new Map<string, AbsentColleague[]>();
    for (const day of days) byDay.set(day.day, day.people);
    return byDay;
  }, [days]);
}

/** The pill text a cramped cell shows: the first name, and how many more. */
export function awayCellText(people: AbsentColleague[]): string {
  const first = people[0];
  if (first === undefined) return "";
  return people.length === 1
    ? first.name
    : `${first.name} +${people.length - 1}`;
}

/** Every name, for the title and the accessible label. */
export function awayNames(people: AbsentColleague[]): string {
  return people.map((person) => person.name).join(", ");
}
