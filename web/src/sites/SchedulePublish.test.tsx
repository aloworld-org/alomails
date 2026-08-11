// Publishing at a chosen moment (S2.05b): the visible control.
//
// What is worth pinning is what a person depends on: the moment is shown in
// their own time with the zone named, the picker offers a real proposal
// instead of an empty field, what they pick is sent as an instant (so the
// server and the screen never disagree about which nine o'clock it was),
// calling it off says so, and a refusal is shown in the server's own words.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { SchedulePublish } from "./SchedulePublish";
import { SitesError } from "./api";
import type { SitePublishSchedule } from "./types";

const mocks = vi.hoisted(() => ({
  publishSchedule: vi.fn(),
  scheduleSitePublish: vi.fn(),
  cancelSitePublish: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, useSitesApi: () => mocks };
});

const MOMENT = "2026-09-01T07:00:00Z";

function schedule(overrides: Partial<SitePublishSchedule> = {}): SitePublishSchedule {
  return {
    id: "sched-1",
    siteId: "site-1",
    publishAt: MOMENT,
    status: "scheduled",
    requestedBy: "user-1",
    createdAt: "2026-08-12T10:00:00Z",
    updatedAt: "2026-08-12T10:00:00Z",
    finishedAt: null,
    attempts: 0,
    publishId: null,
    lastError: null,
    ...overrides,
  };
}

const readable = (iso: string) =>
  new Intl.DateTimeFormat(undefined, {
    dateStyle: "full",
    timeStyle: "short",
  }).format(new Date(iso));

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.publishSchedule.mockResolvedValue({ schedule: null, history: [] });
});

afterEach(cleanup);

test("nothing scheduled invites the person to choose a moment", async () => {
  render(<SchedulePublish siteId="site-1" />);
  expect(await screen.findByText(strings.sitesScheduleHint)).toBeTruthy();
  expect(
    screen.getByRole("button", { name: strings.sitesScheduleOpen }),
  ).toBeTruthy();
});

test("the picker opens on a real proposal and sends the instant it means", async () => {
  const chosen = schedule();
  mocks.scheduleSitePublish.mockResolvedValue(chosen);
  render(<SchedulePublish siteId="site-1" />);

  fireEvent.click(
    await screen.findByRole("button", { name: strings.sitesScheduleOpen }),
  );

  // The field is never blank: it proposes tomorrow morning, and the screen
  // says out loud when that is and in whose time.
  const field = screen.getByLabelText(strings.sitesScheduleWhen);
  const proposed = (field as HTMLInputElement).value;
  expect(proposed).toMatch(/^\d{4}-\d{2}-\d{2}T09:00$/);
  expect(
    screen.getByText(strings.sitesScheduleGoesLive(readable(new Date(proposed).toISOString()))),
  ).toBeTruthy();
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  expect(screen.getByText(strings.sitesScheduleTimeZone(zone))).toBeTruthy();

  fireEvent.change(field, { target: { value: "2026-09-01T09:00" } });
  fireEvent.click(screen.getByRole("button", { name: strings.sitesScheduleSave }));

  await waitFor(() => expect(mocks.scheduleSitePublish).toHaveBeenCalledTimes(1));
  const [siteId, sent] = mocks.scheduleSitePublish.mock.calls[0] as [string, string];
  expect(siteId).toBe("site-1");
  // What was picked is the LOCAL wall clock; what is sent is that instant.
  expect(sent).toBe(new Date("2026-09-01T09:00").toISOString());
});

test("a waiting schedule is stated in the reader's own time, and can be called off", async () => {
  const pending = schedule();
  mocks.publishSchedule.mockResolvedValue({
    schedule: pending,
    history: [pending],
  });
  mocks.cancelSitePublish.mockImplementation(async () => {
    mocks.publishSchedule.mockResolvedValue({
      schedule: null,
      history: [schedule({ status: "cancelled", finishedAt: MOMENT })],
    });
    return schedule({ status: "cancelled", finishedAt: MOMENT });
  });
  render(<SchedulePublish siteId="site-1" />);

  expect(
    await screen.findByText(strings.sitesSchedulePending(readable(MOMENT))),
  ).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: strings.sitesScheduleCancel }));

  await waitFor(() =>
    expect(mocks.cancelSitePublish).toHaveBeenCalledWith("site-1", "sched-1"),
  );
  expect(
    await screen.findByText(strings.sitesScheduleCancelled(readable(MOMENT))),
  ).toBeTruthy();
});

test("a publish that refused is reported in the server's own words", async () => {
  mocks.publishSchedule.mockResolvedValue({
    schedule: null,
    history: [
      schedule({
        status: "failed",
        attempts: 1,
        finishedAt: MOMENT,
        lastError: "site has no home page",
      }),
    ],
  });
  render(<SchedulePublish siteId="site-1" />);

  expect(
    await screen.findByText(
      strings.sitesScheduleFailed(readable(MOMENT), "site has no home page"),
    ),
  ).toBeTruthy();
});

test("a website that published itself says so", async () => {
  mocks.publishSchedule.mockResolvedValue({
    schedule: null,
    history: [
      schedule({
        status: "published",
        attempts: 1,
        finishedAt: MOMENT,
        publishId: "publish-1",
      }),
    ],
  });
  render(<SchedulePublish siteId="site-1" />);

  expect(
    await screen.findByText(strings.sitesScheduleDone(readable(MOMENT))),
  ).toBeTruthy();
});

test("a refused schedule shows the server's sentence, not a generic one", async () => {
  mocks.scheduleSitePublish.mockRejectedValue(
    new SitesError(422, "a scheduled publish must be in the future"),
  );
  render(<SchedulePublish siteId="site-1" />);

  fireEvent.click(
    await screen.findByRole("button", { name: strings.sitesScheduleOpen }),
  );
  fireEvent.change(screen.getByLabelText(strings.sitesScheduleWhen), {
    target: { value: "2020-01-01T09:00" },
  });
  fireEvent.click(screen.getByRole("button", { name: strings.sitesScheduleSave }));

  expect(
    await screen.findByText("a scheduled publish must be in the future"),
  ).toBeTruthy();
});
