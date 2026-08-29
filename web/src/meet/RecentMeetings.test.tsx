// The record surface Meet gained (A8.4): a meeting that has ended can be put
// in focus, and its agent stands under the list — quiet until it is.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import type { Meeting } from "./api";
import { RecentMeetings, meetingOrigin } from "./RecentMeetings";

const fetched: string[] = [];

/** The wire: the directory names one Meet agent, and the room a meeting came
 *  out of has a name to cite. */
const fakeFetch = vi.fn(async (url: string) => {
  fetched.push(url);
  const body = url.includes("/chat/agents/directory")
    ? {
        agents: [
          {
            id: "agent-meet",
            handle: "meet",
            name: "Meet",
            product: "meet",
            disabled: false,
            tools: [
              { name: "meeting_record", effect: "read" },
              { name: "meeting_minutes", effect: "write" },
            ],
          },
        ],
      }
    : { id: "room-1", name: "release" };
  return new Response(JSON.stringify(body), {
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

const ENDED: Meeting = {
  id: "m-1",
  title: "Budget review",
  createdBy: "u-1",
  channel: "room-1",
  event: null,
  createdAt: "2026-08-28T09:00:00Z",
  startedAt: "2026-08-28T09:00:00Z",
  endedAt: "2026-08-28T09:45:00Z",
  live: false,
};

const OTHER: Meeting = { ...ENDED, id: "m-2", title: "Retro", channel: null };

beforeEach(() => {
  fetched.length = 0;
});

afterEach(cleanup);

function show(history: Meeting[] = [ENDED, OTHER]) {
  render(
    <MemoryRouter>
      <RecentMeetings history={history} />
    </MemoryRouter>,
  );
}

test("the list is quiet until a meeting is put in focus", async () => {
  show();
  expect(screen.getByText("Budget review")).toBeTruthy();
  expect(screen.queryByLabelText(strings.recordAgentTitle)).toBeNull();
  // Nothing was read on open: the panel is what reads, and it is not there.
  expect(fetched.length).toBe(0);

  fireEvent.click(
    screen.getByLabelText(strings.recordAgentFocusRecord("Budget review")),
  );
  expect(await screen.findByLabelText(strings.recordAgentTitle)).toBeTruthy();
  // The meeting came out of a conversation, and the panel cites it by name.
  expect(
    await screen.findByText(strings.recordAgentOriginThread("release")),
  ).toBeTruthy();
  expect(
    await screen.findByText(strings.recordAgentVerbMeetingMinutes),
  ).toBeTruthy();
});

test("one meeting is in focus at a time, and the focused one lets go", async () => {
  show();
  const budget = screen.getByLabelText(
    strings.recordAgentFocusRecord("Budget review"),
  );
  const retro = screen.getByLabelText(strings.recordAgentFocusRecord("Retro"));

  fireEvent.click(budget);
  const panel = await screen.findByLabelText(strings.recordAgentTitle);
  expect(panel.getAttribute("data-record")).toBe("meeting:m-1");

  fireEvent.click(retro);
  await waitFor(() =>
    expect(
      screen
        .getByLabelText(strings.recordAgentTitle)
        .getAttribute("data-record"),
    ).toBe("meeting:m-2"),
  );
  expect(screen.getAllByLabelText(strings.recordAgentTitle).length).toBe(1);

  fireEvent.click(retro);
  await waitFor(() =>
    expect(screen.queryByLabelText(strings.recordAgentTitle)).toBeNull(),
  );
});

test("a meeting's origin is what the meeting itself carries, never its creator's id", () => {
  expect(meetingOrigin(ENDED)).toEqual({
    kind: "thread",
    id: "room-1",
    label: null,
  });
  expect(meetingOrigin({ ...ENDED, channel: null, event: "ev-9" })).toEqual({
    kind: "event",
    id: "ev-9",
    label: null,
  });
  // `createdBy` is an account id — unreadable, so not an origin (AW.2).
  expect(meetingOrigin(OTHER)).toBeNull();
});
