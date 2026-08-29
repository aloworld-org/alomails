// The working-hours dialog: it loads the saved schedule into its controls,
// a day toggle and the window edits go back to the server in the wire
// spelling, and a backwards window is refused in place without a request.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import type { WorkingHours } from "../jmap";
import { WorkingHoursDialog } from "./WorkingHoursDialog";

afterEach(cleanup);

const saved: WorkingHours[] = [];

// One client object for the file: the dialog lists `client` in its load
// effect's dependencies, so a fresh identity per render would re-fetch.
const CLIENT = {
  workingHours: async (): Promise<WorkingHours> => ({
    days: [1, 2, 3, 4, 5],
    start: "09:00",
    end: "17:00",
    zone: null,
  }),
  setWorkingHours: async (hours: WorkingHours) => {
    saved.push(hours);
  },
};

vi.mock("../jmap", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useJmapClient: () => CLIENT,
}));

test("the saved schedule arrives in the controls and edits go back on the wire", async () => {
  const onClose = vi.fn();
  render(<WorkingHoursDialog onClose={onClose} />);

  // The default schedule loads: Monday pressed, Saturday not.
  const days = await screen.findAllByRole("button", { pressed: true });
  expect(days).toHaveLength(5);
  const start = screen.getByLabelText<HTMLInputElement>(strings.agendaWorkStart);
  expect(start.value).toBe("09:00");

  // Drop Friday, end at 16:00, follow Brussels.
  const friday = days[4];
  expect(friday).toBeTruthy();
  fireEvent.click(friday as HTMLElement);
  fireEvent.change(screen.getByLabelText(strings.agendaWorkEnd), {
    target: { value: "16:00" },
  });
  fireEvent.change(screen.getByLabelText(strings.agendaWorkZone), {
    target: { value: "Europe/Brussels" },
  });
  fireEvent.click(screen.getByRole("button", { name: strings.agendaSave }));

  await waitFor(() => expect(onClose).toHaveBeenCalled());
  expect(saved).toEqual([
    { days: [1, 2, 3, 4], start: "09:00", end: "16:00", zone: "Europe/Brussels" },
  ]);
});

test("a backwards window is refused in place, without a request", async () => {
  saved.length = 0;
  render(<WorkingHoursDialog onClose={vi.fn()} />);
  await screen.findAllByRole("button", { pressed: true });

  fireEvent.change(screen.getByLabelText(strings.agendaWorkEnd), {
    target: { value: "08:00" },
  });
  fireEvent.click(screen.getByRole("button", { name: strings.agendaSave }));

  expect(await screen.findByText(strings.agendaWorkHoursOrder)).toBeTruthy();
  expect(saved).toHaveLength(0);
});
