// What Calendar gained by adopting `ds/` (D2.05).
//
// Each test names what the hand-built version did instead.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import type { Calendar, CalendarGrant, ShareableGroup } from "../jmap";
import { AgendaModule } from "./AgendaModule";
import { ShareDialog } from "./ShareDialog";

afterEach(cleanup);

const CALENDAR: Calendar = {
  id: "cal-1",
  name: "Team",
  color: "#e76f51",
  kind: "personal",
  role: "owner",
};

const GRANTS: CalendarGrant[] = [
  {
    kind: "user",
    subject: "ada@example.test",
    label: "ada@example.test",
    role: "viewer",
  },
  { kind: "group", subject: "g-1", label: "Marketing", role: "editor" },
];

const GROUPS: ShareableGroup[] = [{ id: "g-1", name: "Marketing" }];

const calls = { unshared: [] as string[] };

// One client object for the file: `ShareDialog` lists `client` in an effect's
// dependencies, so a fresh identity per render would re-fetch forever.
const CLIENT = {
  calendarGrants: async () => GRANTS,
  shareableGroups: async () => GROUPS,
  shareCalendar: async () => undefined,
  unshareCalendar: async (_cal: string, _kind: string, subject: string) => {
    calls.unshared.push(subject);
  },
  calendars: async () => [CALENDAR],
  calendarEvents: async () => [],
  createCalendar: async () => CALENDAR,
  deleteCalendar: async () => undefined,
};

vi.mock("../jmap", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useJmapClient: () => CLIENT,
}));

beforeEach(() => {
  calls.unshared = [];
});

describe("the share dialog is a dialog", () => {
  test("Escape closes it, where nothing in the file handled a key", () => {
    // Before: `div.modalScrim` + `div.modal` with `role="dialog"` written by
    // hand and no key handler at all — the only way out was the mouse.
    let closed = 0;
    render(<ShareDialog calendar={CALENDAR} onClose={() => (closed += 1)} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(closed).toBe(1);
  });

  test("each field is reachable by its label", async () => {
    // Before: `<label class="field"><span>Access</span><select>…` — a wrapping
    // label, which does bind, but only because every one of the four
    // remembered to wrap. `ds/Field` binds by id whether they remember or not.
    render(<ShareDialog calendar={CALENDAR} onClose={() => undefined} />);
    expect(await screen.findByLabelText(strings.agendaShareWith)).toBeTruthy();
    expect(screen.getByLabelText(strings.agendaShareAccess)).toBeTruthy();
    expect(screen.getByLabelText(strings.agendaShareEmail)).toBeTruthy();
  });

  test("choosing to share with a group swaps the email box for a group picker", async () => {
    // The behaviour the migration could most easily have broken: which field
    // is drawn depends on the first select's value.
    render(<ShareDialog calendar={CALENDAR} onClose={() => undefined} />);
    fireEvent.change(await screen.findByLabelText(strings.agendaShareWith), {
      target: { value: "group" },
    });
    const picker = screen.getByLabelText(strings.agendaShareGroupOption);
    expect(picker.tagName).toBe("SELECT");
    // The prompt is still an option and still selectable, because "no group
    // chosen" is a state this form can be in.
    const prompt = screen.getByRole("option", {
      name: strings.agendaShareGroupPick,
    }) as HTMLOptionElement;
    expect(prompt.value).toBe("");
    expect(prompt.disabled).toBe(false);
  });

  test("each row's remove button names whose access it takes away", async () => {
    // Before: a row of buttons all called "Remove", which is what a screen
    // reader reads out — two identical commands and no way to tell them apart.
    render(<ShareDialog calendar={CALENDAR} onClose={() => undefined} />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: strings.agendaShareRemoveFor("Marketing"),
      }),
    );
    await waitFor(() => expect(calls.unshared).toEqual(["g-1"]));
  });
});

describe("the toolbar above the grid", () => {
  test("it is one named group, and every control keeps its own tab stop", async () => {
    // Before: a bare `<header class="toolbar">`. Nothing said the Today
    // button, the two arrows and the view picker belonged together.
    //
    // `keyboard="tab"` rather than `roving`: the row carries a heading and a
    // spinner between its controls, so the arrow keys `role="toolbar"`
    // promises would have nothing coherent to move between.
    render(
      <DialogProvider>
        <AgendaModule />
      </DialogProvider>,
    );
    const bar = await screen.findByRole("group", {
      name: strings.agendaToolbarLabel,
    });
    expect(bar.getAttribute("role")).toBe("group");
    const today = screen.getByRole("button", { name: strings.agendaToday });
    expect(today.getAttribute("tabindex")).toBeNull();
  });

  test("the four view buttons are announced as one choice, with the current one marked", async () => {
    // Before: four bare words in a `div.viewSwitch`, and the only sign of
    // which was current was a background colour.
    render(
      <DialogProvider>
        <AgendaModule />
      </DialogProvider>,
    );
    const group = await screen.findByRole("group", {
      name: strings.agendaViewLabel,
    });
    const month = screen.getByRole("button", { name: strings.agendaMonth });
    expect(group.contains(month)).toBe(true);
    expect(month.getAttribute("aria-current")).toBe("true");
    const week = screen.getByRole("button", { name: strings.agendaWeek });
    expect(week.getAttribute("aria-current")).toBeNull();
    fireEvent.click(week);
    expect(week.getAttribute("aria-current")).toBe("true");
  });

  test("the two arrows are named, not drawn", async () => {
    render(
      <DialogProvider>
        <AgendaModule />
      </DialogProvider>,
    );
    expect(
      await screen.findByRole("button", { name: strings.agendaPrev }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: strings.agendaNext }),
    ).toBeTruthy();
  });
});
