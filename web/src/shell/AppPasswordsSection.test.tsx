// App passwords in Settings: what is worth proving is the lifecycle the
// screen promises — the secret appears exactly once after create and never
// again, the list shows the record (name, created, last used) and not the
// secret, and revoke removes the row immediately.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import type { AppPassword } from "../jmap";
import { AppPasswordsSection } from "./AppPasswordsSection";

afterEach(cleanup);

const state = {
  stored: [] as AppPassword[],
  revoked: [] as string[],
};

vi.mock("../jmap", () => ({
  useJmapClient: () => ({
    listAppPasswords: async () => [...state.stored],
    createAppPassword: async (name: string) => {
      const made = {
        id: `ap-${String(state.stored.length + 1)}`,
        name,
        createdAt: "2026-08-26T09:00:00Z",
        lastUsedAt: null,
      };
      state.stored.push(made);
      return { id: made.id, name, secret: "abcd-efgh-ijkl-mnop" };
    },
    revokeAppPassword: async (id: string) => {
      state.revoked.push(id);
      state.stored = state.stored.filter((p) => p.id !== id);
    },
  }),
}));

describe("AppPasswordsSection", () => {
  beforeEach(() => {
    state.stored = [];
    state.revoked = [];
  });

  test("an empty list explains what app passwords are for", async () => {
    render(<AppPasswordsSection />);
    expect(await screen.findByText(strings.appPasswordNone)).toBeTruthy();
  });

  test("the list shows the record, never a secret", async () => {
    state.stored = [
      {
        id: "ap-1",
        name: "Thunderbird",
        createdAt: "2026-08-01T08:00:00Z",
        lastUsedAt: null,
      },
    ];
    render(<AppPasswordsSection />);
    expect(await screen.findByText("Thunderbird")).toBeTruthy();
    expect(
      screen.getByText(new RegExp(strings.appPasswordNeverUsed)),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", {
        name: strings.appPasswordRevokeFor("Thunderbird"),
      }),
    ).toBeTruthy();
  });

  test("creating shows the secret once, and Done takes it away for good", async () => {
    render(<AppPasswordsSection />);
    await screen.findByText(strings.appPasswordNone);

    fireEvent.change(
      screen.getByLabelText(strings.appPasswordNamePlaceholder),
      { target: { value: "Old phone" } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.appPasswordCreate }),
    );

    // The one appearance: the full secret, with the shown-once warning.
    expect(await screen.findByText("abcd-efgh-ijkl-mnop")).toBeTruthy();
    expect(screen.getByText(strings.appPasswordSecretHint)).toBeTruthy();
    // The list refreshed behind it.
    expect(await screen.findByText("Old phone")).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", { name: strings.appPasswordSecretDone }),
    );
    expect(screen.queryByText("abcd-efgh-ijkl-mnop")).toBeNull();
  });

  test("revoke removes the row immediately", async () => {
    state.stored = [
      {
        id: "ap-9",
        name: "Old laptop",
        createdAt: "2026-08-01T08:00:00Z",
        lastUsedAt: "2026-08-20T10:00:00Z",
      },
    ];
    render(<AppPasswordsSection />);
    await screen.findByText("Old laptop");

    fireEvent.click(
      screen.getByRole("button", {
        name: strings.appPasswordRevokeFor("Old laptop"),
      }),
    );
    await waitFor(() => {
      expect(screen.queryByText("Old laptop")).toBeNull();
    });
    expect(state.revoked).toEqual(["ap-9"]);
  });
});
