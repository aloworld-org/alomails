// The meeting's agent in the event editor (AS.6): on a screen too narrow for
// the day panel — where the meeting in focus normally lives — the editor is the
// record's only surface, so it mounts the same panel. On a wide screen it must
// not, or the same agent appears twice on one meeting; and an event that has
// never been saved has no record to ask about.
//
// The panel carries its own `<form>`, which is why the editor's fields moved
// into a named form of their own: the last test here is the one that keeps
// saving working after that change.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import type { Calendar, CalendarEvent, EventInput } from "../jmap";
import { EventModal } from "./EventModal";

const DIRECTORY = {
  agents: [
    {
      id: "agent-agenda",
      handle: "agenda",
      name: "Agenda",
      product: "agenda",
      disabled: false,
      tools: [
        { name: "meeting_prep", effect: "read" },
        { name: "reschedule_event", effect: "write" },
      ],
    },
  ],
};

const fakeFetch = vi.fn(
  async () =>
    new Response(JSON.stringify(DIRECTORY), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
);

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

vi.mock("react-router-dom", () => ({
  useNavigate: () => vi.fn(),
}));

const CLIENT = {
  calendarResources: async () => [],
  freeBusy: async () => [],
};

vi.mock("../jmap", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useJmapClient: () => CLIENT,
}));

vi.mock("../meet", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useMeetApi: () => ({
    forEvent: async () => null,
    start: async () => null,
  }),
}));

/** A matchMedia that answers the given width against the one query this
 *  component asks — the day panel's own breakpoint. */
function atWidth(px: number) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: px <= Number(/(\d+)px/.exec(query)?.[1] ?? "0"),
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
}

const CALENDARS: Calendar[] = [
  {
    id: "cal_personal_u1",
    name: "Personal",
    color: null,
    kind: "personal",
    role: "owner",
  },
];

const EVENT: CalendarEvent = {
  id: "ev1",
  calendarId: "cal_personal_u1",
  summary: "Delaunay review",
  description: null,
  location: null,
  startsAt: "2026-09-02T10:00:00.000Z",
  endsAt: "2026-09-02T11:00:00.000Z",
  allDay: false,
  recurrence: null,
  attendees: [],
  recurrenceId: null,
  reminderMinutes: null,
  attendeeStatus: [],
};

function modal(
  event: CalendarEvent | null,
  onSave: (id: string | null, input: EventInput) => Promise<void> = async () =>
    undefined,
) {
  render(
    <EventModal
      event={event}
      initialStart={new Date("2026-09-02T10:00:00")}
      calendars={CALENDARS}
      onSave={onSave}
      onDelete={async () => undefined}
      onClose={vi.fn()}
    />,
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
  fakeFetch.mockClear();
  cleanup();
});

test("a phone opens a saved meeting with its agent under the form", async () => {
  atWidth(360);
  modal(EVENT);

  expect(await screen.findByText(strings.recordAgentTitle)).toBeTruthy();
  // The verbs are the registry's, filtered to what this agent actually has —
  // the panel's own doing, and the proof it is the panel and not a copy.
  expect(
    await screen.findByText(strings.recordAgentVerbMeetingPrep),
  ).toBeTruthy();
  expect(screen.getByText(strings.recordAgentVerbRescheduleEvent)).toBeTruthy();
  // A calendar event carries no source of its own, and the panel says so.
  expect(screen.getByText(strings.recordAgentOriginNone)).toBeTruthy();
});

test("a wide screen leaves the editor as it was — the day panel has the agent", async () => {
  atWidth(1280);
  modal(EVENT);

  await screen.findByPlaceholderText(strings.agendaEventTitle);
  expect(screen.queryByText(strings.recordAgentTitle)).toBeNull();
  // Nothing is read for a panel that is not there.
  expect(fakeFetch).not.toHaveBeenCalled();
});

test("an unsaved event has no record to ask about", async () => {
  atWidth(360);
  modal(null);

  await screen.findByPlaceholderText(strings.agendaEventTitle);
  expect(screen.queryByText(strings.recordAgentTitle)).toBeNull();
  expect(fakeFetch).not.toHaveBeenCalled();
});

test("the fields still save while the panel is mounted beside them", async () => {
  atWidth(360);
  const saved: EventInput[] = [];
  modal(EVENT, async (_id, input) => {
    saved.push(input);
  });

  await screen.findByText(strings.recordAgentTitle);
  fireEvent.change(screen.getByPlaceholderText(strings.agendaEventTitle), {
    target: { value: "Delaunay review, moved" },
  });
  fireEvent.click(screen.getByRole("button", { name: strings.agendaSave }));

  await waitFor(() => expect(saved).toHaveLength(1));
  expect(saved[0]?.summary).toBe("Delaunay review, moved");
});
