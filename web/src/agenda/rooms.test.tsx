// Booking a room from the event editor: the workspace's rooms arrive in a
// picker, the chosen one rides out as an attendee (booking is naming), a room
// already on the event is shown in the picker rather than in the guest box,
// and a refused save says which room is taken rather than "couldn't save".
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { JmapError } from "../jmap";
import type { Calendar, CalendarEvent, EventInput } from "../jmap";
import { EventModal } from "./EventModal";

afterEach(cleanup);

const ROOMS = [
  {
    id: "cal_room_1",
    name: "Board room",
    email: "board@example.test",
    location: "2nd floor",
    capacity: 8,
  },
  {
    id: "cal_room_2",
    name: "Huddle",
    email: "huddle@example.test",
    location: null,
    capacity: null,
  },
];

// One client object for the file: the modal lists `client` in its load
// effect's dependencies, so a fresh identity per render would re-fetch.
const CLIENT = {
  calendarResources: async () => ROOMS,
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

const CALENDARS: Calendar[] = [
  {
    id: "cal_personal_u1",
    name: "Personal",
    color: null,
    kind: "personal",
    role: "owner",
  },
];

function event(attendees: string[]): CalendarEvent {
  return {
    id: "ev1",
    calendarId: "cal_personal_u1",
    summary: "Board meeting",
    description: null,
    location: null,
    startsAt: "2026-09-02T10:00:00.000Z",
    endsAt: "2026-09-02T11:00:00.000Z",
    allDay: false,
    recurrence: null,
    attendees,
    recurrenceId: null,
    reminderMinutes: null,
    attendeeStatus: [],
  };
}

test("picking a room sends it out with the meeting", async () => {
  const saved: EventInput[] = [];
  render(
    <EventModal
      event={null}
      initialStart={new Date("2026-09-02T10:00:00")}
      calendars={CALENDARS}
      onSave={async (_id, input) => {
        saved.push(input);
      }}
      onDelete={async () => undefined}
      onClose={vi.fn()}
    />,
  );

  // The picker shows what each room is, so choosing needs no second screen.
  const picker = await screen.findByRole<HTMLSelectElement>("combobox", {
    name: strings.agendaRoom,
  });
  expect(
    screen.getByRole("option", { name: "Board room — 2nd floor, seats 8" }),
  ).toBeTruthy();
  expect(screen.getByRole("option", { name: "Huddle" })).toBeTruthy();

  fireEvent.change(screen.getByPlaceholderText(strings.agendaEventTitle), {
    target: { value: "Board meeting" },
  });
  fireEvent.change(picker, { target: { value: "cal_room_1" } });
  fireEvent.click(
    screen.getByRole("button", { name: strings.agendaCreateEvent }),
  );

  await waitFor(() => expect(saved).toHaveLength(1));
  expect(saved[0]?.attendees).toEqual(["board@example.test"]);
});

test("a room already on the event shows in the picker, not among the guests", async () => {
  render(
    <EventModal
      event={event(["colleague@example.test", "board@example.test"])}
      initialStart={new Date("2026-09-02T10:00:00")}
      calendars={CALENDARS}
      onSave={async () => undefined}
      onDelete={async () => undefined}
      onClose={vi.fn()}
    />,
  );

  const picker = await screen.findByRole<HTMLSelectElement>("combobox", {
    name: strings.agendaRoom,
  });
  await waitFor(() => expect(picker.value).toBe("cal_room_1"));
  const guests = screen.getByPlaceholderText<HTMLInputElement>(
    strings.agendaGuestsPlaceholder,
  );
  expect(guests.value).toBe("colleague@example.test");
});

test("a taken room is named in place of a bare save failure", async () => {
  render(
    <EventModal
      event={null}
      initialStart={new Date("2026-09-02T10:00:00")}
      calendars={CALENDARS}
      onSave={async () => {
        throw new JmapError("calendar create 409", 409);
      }}
      onDelete={async () => undefined}
      onClose={vi.fn()}
    />,
  );

  const picker = await screen.findByRole<HTMLSelectElement>("combobox", {
    name: strings.agendaRoom,
  });
  fireEvent.change(screen.getByPlaceholderText(strings.agendaEventTitle), {
    target: { value: "Board meeting" },
  });
  fireEvent.change(picker, { target: { value: "cal_room_1" } });
  fireEvent.click(
    screen.getByRole("button", { name: strings.agendaCreateEvent }),
  );

  expect(
    await screen.findByText(strings.agendaRoomTaken("Board room")),
  ).toBeTruthy();
});
