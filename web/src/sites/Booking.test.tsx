// What the booking screens must keep doing (S2.13c): a site whose account has
// no calendar is told that, rather than shown a picker with nothing in it; a
// service whose calendar has gone says so instead of quietly offering no times;
// a new service is created with the whole shape the route expects, in one write;
// the server's own refusal sentence is what a person reads; and a page section
// can only offer a service that exists.
//
// Same harness as PageEditor.test.tsx: the real API client and the real views
// run, and only the network is faked, so the URLs and bodies asserted here are
// the ones the wire-verified routes take.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SitesModule } from "./SitesModule";
import { SectionFormDialog } from "./SectionForm";
import { DEFAULT_SECTION_PRESENTATION } from "./sectionDrafts";
import { suggestFieldKey, timeMinutes, timeValue, windowLabel } from "./bookingSchedule";
import type { SiteBooking } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

interface Reply {
  match: (url: string, method: string) => boolean;
  status: number;
  body: unknown;
}

const calls: Call[] = [];
let replies: Reply[] = [];

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const index = replies.findIndex((reply) => reply.match(url, method));
  const answer =
    index === -1 ? { status: 200, body: {} } : (replies.splice(index, 1)[0] as Reply);
  return new Response(JSON.stringify(answer.body), {
    status: answer.status,
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

const CALENDAR = { id: "cal-1", name: "Consultations", writable: true };
const READ_ONLY = { id: "cal-2", name: "Team leave", writable: false };

const CONSULTATION: SiteBooking = {
  id: "book-1",
  name: "Thirty-minute consultation",
  description: "A first conversation.",
  calendarId: "cal-1",
  calendar: CALENDAR,
  timeZone: "Europe/Brussels",
  durationMinutes: 30,
  bufferMinutes: 10,
  noticeMinutes: 120,
  horizonDays: 60,
  location: "Second floor",
  hours: [{ weekday: 1, startMinute: 540, endMinute: 1020 }],
  fields: [],
  active: true,
  createdAt: "2026-08-13T08:00:00Z",
  updatedAt: "2026-08-13T08:00:00Z",
};

/** The same service after its calendar was deleted or unshared: the id is
 *  still stored, but the account can no longer resolve it. */
const ORPHANED: SiteBooking = {
  ...CONSULTATION,
  id: "book-2",
  name: "Site visit",
  calendarId: "cal-gone",
  calendar: null,
};

function sourcesReply(sources: unknown[]): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/booking-sources"),
    status: 200,
    body: { sources },
  };
}

function bookingsReply(bookings: SiteBooking[]): Reply {
  return {
    match: (url, method) => method === "GET" && url.endsWith("/sites/site-1/bookings"),
    status: 200,
    body: { bookings },
  };
}

function ui(path = "/sites/site-1/bookings") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/sites/*" element={<SitesModule />} />
        <Route path="/agenda" element={<p>Agenda</p>} />
      </Routes>
    </MemoryRouter>,
  );
}

function lastWrite(): Call | undefined {
  return calls.filter((call) => call.method !== "GET").at(-1);
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the week a service is offered in", () => {
  test("a window survives the trip through the form's time fields", () => {
    const window = { weekday: 3, startMinute: 9 * 60 + 30, endMinute: 17 * 60 };
    expect(timeValue(window.startMinute)).toBe("09:30");
    expect(timeMinutes(timeValue(window.startMinute))).toBe(window.startMinute);
    // Midnight as an end is the whole day, not minute zero of the next one.
    expect(timeValue(24 * 60)).toBe("24:00");
    expect(timeMinutes("24:00")).toBe(24 * 60);
    // A half-typed or nonsense time is refused rather than read as an hour
    // nobody meant — the caller keeps the window it had.
    expect(timeMinutes("")).toBeNull();
    expect(timeMinutes("9")).toBeNull();
    expect(timeMinutes("25:00")).toBeNull();
    expect(timeMinutes("09:75")).toBeNull();
    expect(windowLabel(window)).toContain("09:30–17:00");
  });

  test("a question's key is suggested from its label, never invented from nothing", () => {
    expect(suggestFieldKey("Telephone number")).toBe("telephone_number");
    expect(suggestFieldKey("Numéro de téléphone")).toBe("numero_de_telephone");
    expect(suggestFieldKey("  ¿Qué? ")).toBe("que");
    expect(suggestFieldKey("")).toBe("");
  });
});

describe("the bookings screen", () => {
  test("an account with no calendar is told the dependency, not shown an empty picker", async () => {
    replies = [sourcesReply([]), bookingsReply([])];

    ui();

    expect(await screen.findByText(strings.sitesBookingNoCalendarTitle)).toBeTruthy();
    expect(screen.getByText(strings.sitesBookingNoCalendarBody)).toBeTruthy();
    // Nothing can be created until Agenda has a calendar, so nothing offers to.
    expect(screen.queryByRole("button", { name: strings.sitesNewBooking })).toBeNull();
  });

  test("a site with a calendar and nothing to book invites the first service", async () => {
    replies = [sourcesReply([CALENDAR]), bookingsReply([])];

    ui();

    expect(await screen.findByText(strings.sitesBookingNoneTitle)).toBeTruthy();
    expect(screen.getByText(strings.sitesBookingNoneBody)).toBeTruthy();
    // The create form is already open, on the working week: a first service is
    // one name away from being offerable, not a form somebody has to find.
    expect(screen.getByRole("button", { name: strings.sitesBookingCreate })).toBeTruthy();
    expect(screen.getAllByDisplayValue("09:00")).toHaveLength(5);
  });

  test("a new service is created in one write, with the whole week it offers", async () => {
    replies = [sourcesReply([CALENDAR, READ_ONLY]), bookingsReply([])];

    ui();

    const name = await screen.findByLabelText(strings.sitesBookingName);
    fireEvent.change(name, { target: { value: "Thirty-minute consultation" } });

    replies = [
      {
        match: (url, method) => method === "POST" && url.endsWith("/sites/site-1/bookings"),
        status: 200,
        body: CONSULTATION,
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesBookingCreate }));

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()).toMatchObject({
      method: "POST",
      body: {
        name: "Thirty-minute consultation",
        // The writable calendar, chosen for the owner: a read-only share is
        // offered in the picker but is never the default the server refuses.
        calendarId: "cal-1",
        durationMinutes: 30,
        horizonDays: 60,
        active: true,
        fields: [],
        hours: [
          { weekday: 1, startMinute: 540, endMinute: 1020 },
          { weekday: 2, startMinute: 540, endMinute: 1020 },
          { weekday: 3, startMinute: 540, endMinute: 1020 },
          { weekday: 4, startMinute: 540, endMinute: 1020 },
          { weekday: 5, startMinute: 540, endMinute: 1020 },
        ],
      },
    });
  });

  test("a rule the store names is read verbatim, with everything typed still there", async () => {
    replies = [sourcesReply([CALENDAR]), bookingsReply([CONSULTATION])];

    ui();

    const name = await screen.findByLabelText(strings.sitesBookingName);
    fireEvent.change(name, { target: { value: "A very long consultation" } });
    replies = [
      {
        match: (url, method) => method === "PUT" && url.endsWith("/bookings/book-1"),
        status: 422,
        body: { detail: "appointment length must be between 5 and 480" },
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesBookingSave }));

    expect(
      await screen.findByText("appointment length must be between 5 and 480"),
    ).toBeTruthy();
    expect(screen.getByLabelText(strings.sitesBookingName)).toHaveProperty(
      "value",
      "A very long consultation",
    );
  });

  test("a service whose calendar has gone says so rather than offering no times", async () => {
    replies = [sourcesReply([CALENDAR]), bookingsReply([CONSULTATION, ORPHANED])];

    ui();

    const list = await screen.findByRole("complementary", { name: strings.sitesBookings });
    fireEvent.click(within(list).getByRole("button", { name: /Site visit/ }));

    expect(screen.getByRole("alert").textContent).toBe(
      strings.sitesBookingCalendarGoneHint,
    );
    // And the way to the place the appointments are actually managed is on the
    // same panel, not somewhere the owner has to remember.
    expect(
      screen.getByRole("link", { name: strings.sitesBookingOpenAgenda }),
    ).toHaveProperty("pathname", "/agenda");
  });

  test("a question is named once: the key follows the label until somebody writes one", async () => {
    replies = [sourcesReply([CALENDAR]), bookingsReply([CONSULTATION])];

    ui();

    fireEvent.click(await screen.findByRole("button", { name: strings.sitesBookingAddQuestion }));
    fireEvent.change(screen.getByLabelText(strings.sitesBookingQuestionLabel), {
      target: { value: "Telephone number" },
    });
    const key = screen.getByLabelText(strings.sitesBookingQuestionKey);
    expect(key).toHaveProperty("value", "telephone_number");

    // Once a key is written by hand it is the answers' address and stops
    // following the label, which a rename would otherwise orphan.
    fireEvent.change(key, { target: { value: "phone" } });
    fireEvent.change(screen.getByLabelText(strings.sitesBookingQuestionLabel), {
      target: { value: "Mobile number" },
    });
    expect(screen.getByLabelText(strings.sitesBookingQuestionKey)).toHaveProperty(
      "value",
      "phone",
    );

    replies = [
      {
        match: (url, method) => method === "PUT" && url.endsWith("/bookings/book-1"),
        status: 200,
        body: CONSULTATION,
      },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.sitesBookingSave }));
    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()).toMatchObject({
      method: "PUT",
      body: {
        fields: [
          { key: "phone", label: "Mobile number", kind: "text", required: false, options: [] },
        ],
      },
    });
  });
});

describe("offering a booking on a page", () => {
  function section(onSave = vi.fn()) {
    return render(
      <MemoryRouter initialEntries={["/sites/site-1/pages/page-1"]}>
        <Routes>
          <Route
            path="/sites/:siteId/pages/:pageId"
            element={
              <SectionFormDialog
                kind="booking"
                busy={false}
                error={null}
                onClose={vi.fn()}
                onSave={onSave}
              />
            }
          />
          <Route path="/sites/:siteId/bookings" element={<p>Bookings</p>} />
        </Routes>
      </MemoryRouter>,
    );
  }

  test("a site with nothing to book explains what a service is and where to make one", async () => {
    replies = [bookingsReply([])];

    section();

    expect(await screen.findByText(strings.sitesBookingSectionNoServices)).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.sitesNewBooking })).toBeTruthy();
    // A section naming nothing would refuse the next publish, so it cannot be
    // saved at all.
    expect(screen.getByRole("button", { name: strings.sitesSaveSection })).toHaveProperty(
      "disabled",
      true,
    );
  });

  test("the section names a service, and a switched-off one says so before it is published", async () => {
    const onSave = vi.fn();
    replies = [bookingsReply([CONSULTATION, { ...ORPHANED, calendar: CALENDAR, active: false }])];

    section(onSave);

    const choose = await screen.findByLabelText(strings.sitesBookingSectionChoose);
    // The first service is chosen for the owner, so the line under the picker
    // describes a real service rather than an empty choice.
    expect(await screen.findByText(strings.sitesBookingSectionLength(30))).toBeTruthy();

    fireEvent.change(choose, { target: { value: "book-2" } });
    expect(await screen.findByText(strings.sitesBookingSectionOff)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: strings.sitesSaveSection }));
    expect(onSave).toHaveBeenCalledWith({
      type: "booking",
      booking_id: "book-2",
      heading: undefined,
      layout: "card",
      presentation: DEFAULT_SECTION_PRESENTATION,
    });
  });
});
