// Out-of-office scheduling on the settings screen.
//
// The dates are the whole feature — "on" and "off" was never how anybody uses
// an auto-reply — so what is worth a build is that a holiday typed here
// arrives at the server as typed, comes back as typed, and that a window
// nobody could mean is refused before a save is attempted.
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
import { SettingsModal } from "./SettingsModal";

afterEach(cleanup);

/** What the server holds, and what the screen last sent it. */
const state = {
  stored: {
    enabled: true,
    subject: "Away",
    message: "Back on the 15th",
    from: null as string | null,
    to: null as string | null,
  },
  sent: null as
    | {
        enabled: boolean;
        subject: string;
        message: string;
        from: string | null;
        to: string | null;
      }
    | null,
};

vi.mock("../jmap", () => ({
  useJmapClient: () => ({
    mailSettings: async () => ({
      signature: "",
      orgFooter: "",
      outOfOffice: state.stored,
    }),
    setOutOfOffice: async (
      enabled: boolean,
      subject: string,
      message: string,
      from: string | null,
      to: string | null,
    ) => {
      state.sent = { enabled, subject, message, from, to };
    },
    setSignature: async () => undefined,
    setOrgFooter: async () => undefined,
    filters: async () => [],
    mailboxes: async () => [],
    saveFilters: async () => [],
    myDelegates: async () => [],
    ownAccountId: async () => "acct-1",
    mailboxesFor: async () => [],
    shareMyMailbox: async () => undefined,
    unshareMyMailbox: async () => undefined,
  }),
}));

beforeEach(() => {
  state.stored = {
    enabled: true,
    subject: "Away",
    message: "Back on the 15th",
    from: null,
    to: null,
  };
  state.sent = null;
});

function mount() {
  return render(
    <DialogProvider>
      <SettingsModal isAdmin={false} onClose={() => undefined} />
    </DialogProvider>,
  );
}

/** The two date inputs, once the screen has loaded its settings. */
async function dateInputs() {
  const from = await screen.findByLabelText(strings.settingsOooFrom);
  const to = await screen.findByLabelText(strings.settingsOooTo);
  return { from, to };
}

describe("out-of-office is a schedule, not a switch", () => {
  test("a holiday typed here reaches the server as the days typed", async () => {
    mount();
    const { from, to } = await dateInputs();
    fireEvent.change(from, { target: { value: "2026-09-01" } });
    fireEvent.change(to, { target: { value: "2026-09-15" } });
    fireEvent.click(screen.getByRole("button", { name: strings.settingsSave }));

    await waitFor(() => expect(state.sent).not.toBeNull());
    expect(state.sent?.from).toBe("2026-09-01");
    expect(state.sent?.to).toBe("2026-09-15");
  });

  test("a stored holiday comes back into the fields as the same days", async () => {
    // The end is stored exclusively — the first moment of the day you are back
    // — and the server hands back the last day away. If either side of that
    // ever drifts, the date somebody typed comes back a day out.
    state.stored = { ...state.stored, from: "2026-09-01", to: "2026-09-15" };
    mount();
    const { from, to } = await dateInputs();
    expect((from as HTMLInputElement).value).toBe("2026-09-01");
    expect((to as HTMLInputElement).value).toBe("2026-09-15");
  });

  test("blank dates mean now until switched off, and send null", async () => {
    // This is what every account had before scheduling existed, and it has to
    // stay reachable: leaving both empty must not become "no holiday".
    mount();
    await dateInputs();
    fireEvent.click(screen.getByRole("button", { name: strings.settingsSave }));

    await waitFor(() => expect(state.sent).not.toBeNull());
    expect(state.sent?.enabled).toBe(true);
    expect(state.sent?.from).toBeNull();
    expect(state.sent?.to).toBeNull();
  });

  test("an end before the start is refused here, not by a save error", async () => {
    // The server refuses it too, but a generic "couldn't save your settings"
    // leaves the person looking at the two fields that caused it without
    // being told which they were.
    mount();
    const { from, to } = await dateInputs();
    fireEvent.change(from, { target: { value: "2026-09-15" } });
    fireEvent.change(to, { target: { value: "2026-09-01" } });
    fireEvent.click(screen.getByRole("button", { name: strings.settingsSave }));

    expect((await screen.findByRole("alert")).textContent).toBe(
      strings.settingsOooBadWindow,
    );
    expect(state.sent).toBeNull();
  });

  test("the dates are hidden while the reply is switched off", async () => {
    // Nothing to schedule when there is no reply: the fields would be three
    // controls to read past on the way to the switch.
    state.stored = { ...state.stored, enabled: false };
    mount();
    await screen.findByLabelText(strings.settingsOooToggle);
    expect(screen.queryByLabelText(strings.settingsOooFrom)).toBeNull();
  });
});
