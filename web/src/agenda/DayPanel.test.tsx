// The day panel's record in focus (A8.4): one meeting at a time can be put in
// focus from the list, which shows that meeting's agent under it — and the
// entry itself still opens the event editor, exactly as before.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import type { CalendarEvent } from "../jmap";
import { DayPanel } from "./DayPanel";

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
        { name: "cancel_event", effect: "write" },
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

const DAY = new Date("2026-08-12T09:00:00Z");

function event(id: string, summary: string, startsAt: string): CalendarEvent {
  return {
    id,
    calendarId: "cal-1",
    summary,
    description: null,
    location: null,
    startsAt,
    endsAt: startsAt,
    allDay: false,
    recurrence: null,
    attendees: [],
    recurrenceId: null,
    reminderMinutes: null,
    attendeeStatus: [],
  };
}

function panel(onEventClick = vi.fn()) {
  render(
    <DayPanel
      day={DAY}
      today={DAY}
      events={[event("ev-1", "Delaunay review", "2026-08-12T10:00:00Z")]}
      absences={new Map()}
      colorOf={() => "#000"}
      onEventClick={onEventClick}
    />,
  );
  return onEventClick;
}

beforeEach(() => {
  fakeFetch.mockClear();
});

afterEach(cleanup);

test("the panel is quiet until a meeting is put in focus", async () => {
  panel();

  // Nothing is read on open: no agent, no verbs, no directory call.
  expect(screen.queryByText(strings.recordAgentTitle)).toBeNull();
  expect(fakeFetch).not.toHaveBeenCalled();

  fireEvent.click(
    screen.getByLabelText(strings.recordAgentFocusRecord("Delaunay review")),
  );

  expect(await screen.findByText(strings.recordAgentVerbMeetingPrep)).toBeTruthy();
  expect(screen.getByText(strings.recordAgentVerbRescheduleEvent)).toBeTruthy();
  expect(screen.getByText(strings.recordAgentVerbCancelEvent)).toBeTruthy();
  // A calendar event carries no source of its own, and the panel says so
  // rather than inventing one.
  expect(screen.getByText(strings.recordAgentOriginNone)).toBeTruthy();
});

test("focusing a meeting does not open it, and the entry still does", () => {
  const onEventClick = panel();

  fireEvent.click(
    screen.getByLabelText(strings.recordAgentFocusRecord("Delaunay review")),
  );
  expect(onEventClick).not.toHaveBeenCalled();

  fireEvent.click(screen.getByText("Delaunay review"));
  expect(onEventClick).toHaveBeenCalledTimes(1);
});

test("the focus is one meeting at a time, and it lets go", async () => {
  panel();
  const toggle = screen.getByLabelText(
    strings.recordAgentFocusRecord("Delaunay review"),
  );

  fireEvent.click(toggle);
  expect(await screen.findByText(strings.recordAgentTitle)).toBeTruthy();

  fireEvent.click(toggle);
  expect(screen.queryByText(strings.recordAgentTitle)).toBeNull();
});
