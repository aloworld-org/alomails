// The workspace's absence feed for the calendar (B7.03): `GET /hr/absences`
// poured into the seam Agenda declares in `agenda/absences.ts`.
//
// It lives on the HR side of the seam because Agenda is shared with the
// standalone mail product, which has no HR — the shared module declares the
// context, the product that owns the data provides it (the `ProductRailWidget`
// reasoning). Imported by the workplace surface directly by file, like
// `ApprovalsWidget`, so the main bundle takes this adapter and not the whole
// HR module behind `hr/index.ts`.
import { useMemo, type ReactNode } from "react";

import { AbsenceLayerContext, type AbsenceSource } from "../agenda/absences";
import { useHrApi } from "./api";

/** Wraps the calendar so its absence layer reads the tenant's approved leave.
 *  The layer discloses what the feed does — a name, an employee id, a day —
 *  and failure is the layer's absence, never the calendar's (the screen with
 *  the error banner is People → Who's away). */
export function AgendaAbsenceProvider({ children }: { children: ReactNode }) {
  const api = useHrApi();
  const source = useMemo<AbsenceSource>(
    () => (from, to) =>
      api.absences(from, to).then((days) =>
        days.map((day) => ({
          day: day.day,
          people: day.people.map((person) => ({
            id: person.employeeId,
            name: person.name,
          })),
        })),
      ),
    [api],
  );
  return (
    <AbsenceLayerContext.Provider value={source}>
      {children}
    </AbsenceLayerContext.Provider>
  );
}
