// The absence layer reaches the calendar (B7.03).
//
// Three facts, each the one its product depends on: an approved absence is
// drawn on its days in the month grid and named in the day panel; an absence
// the feed no longer serves (withdrawn, cancelled) is gone on the next look;
// and the standalone mail product — which mounts the same module with no
// provider — draws no layer at all. Tenancy is the feed's, proven where the
// feed is (`hr_leave_requests_tenancy.rs`): a layer can only draw what
// `/hr/absences` serves, and that is tenant-bound by construction.
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import type { Calendar } from "../jmap";
import { AgendaModule } from "./AgendaModule";
import {
  AbsenceLayerContext,
  localDayKey,
  type AbsenceDay,
  type AbsenceSource,
} from "./absences";

afterEach(cleanup);

const CALENDAR: Calendar = {
  id: "cal-1",
  name: "Team",
  color: "#e76f51",
  kind: "personal",
  role: "owner",
};

// One client object for the file: the module lists `client` in effect
// dependencies, so a fresh identity per render would re-fetch forever.
const CLIENT = {
  calendars: async () => [CALENDAR],
  calendarEvents: async () => [],
};

vi.mock("../jmap", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useJmapClient: () => CLIENT,
}));

// The feed as the test controls it: what it serves, and what it was asked.
const feed = { days: [] as AbsenceDay[], asked: [] as string[] };
const source: AbsenceSource = async (from, to) => {
  feed.asked.push(`${from}..${to}`);
  return feed.days;
};

beforeEach(() => {
  feed.days = [];
  feed.asked = [];
});

function renderWithLayer() {
  return render(
    <AbsenceLayerContext.Provider value={source}>
      <DialogProvider>
        <AgendaModule />
      </DialogProvider>
    </AbsenceLayerContext.Provider>,
  );
}

const today = localDayKey(new Date());

describe("the absence layer in the calendar", () => {
  test("an approved absence appears on its day, named", async () => {
    feed.days = [
      {
        day: today,
        people: [
          { id: "emp-1", name: "Inès Dupont" },
          { id: "emp-2", name: "Jonas Peeters" },
        ],
      },
    ];
    renderWithLayer();
    // The month cell shows the first name and counts the rest…
    expect(await screen.findByText("Inès Dupont +1")).toBeTruthy();
    // …every name is in the pill's title, so nobody is only a number…
    expect(
      screen.getByTitle(strings.agendaAwayTitle("Inès Dupont, Jonas Peeters")),
    ).toBeTruthy();
    // …and the day panel (today is the selected day) spells them all out.
    expect(screen.getByText("Inès Dupont, Jonas Peeters")).toBeTruthy();
    expect(screen.getByText(strings.agendaAway)).toBeTruthy();
  });

  test("an absence the feed no longer serves is gone on the next look", async () => {
    // The feed answers empty — a request withdrawn or cancelled leaves
    // `status = 'approved'` and with it this feed, which is the layer's whole
    // source of truth: nothing is cached, nothing is written into a calendar.
    renderWithLayer();
    await waitFor(() => expect(feed.asked.length).toBeGreaterThan(0));
    expect(screen.queryByText(/Inès Dupont/)).toBeNull();
    expect(screen.queryByText(strings.agendaAway)).toBeNull();
  });

  test("the same module with no provider draws no layer", async () => {
    // The standalone mail product mounts Agenda without a provider: the
    // calendar renders whole, and the layer simply does not exist.
    feed.days = [
      { day: today, people: [{ id: "emp-1", name: "Inès Dupont" }] },
    ];
    render(
      <DialogProvider>
        <AgendaModule />
      </DialogProvider>,
    );
    expect(
      await screen.findByRole("group", { name: strings.agendaToolbarLabel }),
    ).toBeTruthy();
    expect(feed.asked).toHaveLength(0);
    expect(screen.queryByText(/Inès Dupont/)).toBeNull();
  });
});
