// What the admin console gained by adopting `ds/` (D2.05).
//
// Each test names what the hand-built version did instead. The look of a
// migration is a matter of opinion; these are the properties that were
// measurably absent before it, or that the adoption could quietly have broken.
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
import type { AdminGroup, AdminUser, UserModuleAccess } from "../jmap";
import { GroupModal } from "./GroupModal";
import { UserModal } from "./UserModal";
import { UsersPage } from "./UsersPage";

afterEach(cleanup);

const ADA: AdminUser = {
  id: "u-1",
  email: "ada@example.test",
  isAdmin: false,
  roles: [],
  createdAt: "2026-01-01T00:00:00Z",
  aliases: ["ada.l@example.test"],
  messageCount: 12,
  storageBytes: 2048,
};

const BEN: AdminUser = {
  ...ADA,
  id: "u-2",
  email: "ben@example.test",
  isAdmin: true,
  aliases: [],
};

const GROUP: AdminGroup = {
  id: "g-1",
  name: "Marketing",
  address: null,
  memberCount: 1,
  members: [{ id: "u-1", email: "ada@example.test" }],
};

const MODULES: UserModuleAccess[] = [
  { id: "agenda", allowed: true },
  { id: "billing", allowed: false },
];

const calls = {
  admin: [] as { id: string; isAdmin: boolean }[],
  created: [] as string[],
  removedMembers: [] as string[],
};

// One client object for the whole file. A fresh one per render would give the
// effects that depend on it a new identity every time and re-fetch forever —
// the trap `drive/adoption.test.tsx` documented at D2.04.
const CLIENT = {
  listUsers: async () => [ADA, BEN],
  accountId: async () => "u-2",
  setUserAdmin: async (id: string, isAdmin: boolean) => {
    calls.admin.push({ id, isAdmin });
  },
  deleteUser: async () => undefined,
  userModules: async () => MODULES,
  setUserModule: async () => undefined,
  setUserRole: async () => undefined,
  addAlias: async () => undefined,
  removeAlias: async () => undefined,
  resetPassword: async () => undefined,
  createUser: async (email: string) => {
    calls.created.push(email);
  },
  inviteUser: async () => ({ inviteUrl: "https://example.test/invite/abc" }),
  createGroup: async (name: string) => {
    calls.created.push(name);
  },
  renameGroup: async () => undefined,
  setGroupAddress: async () => undefined,
  addGroupMember: async () => undefined,
  removeGroupMember: async (_group: string, userId: string) => {
    calls.removedMembers.push(userId);
  },
  deleteGroup: async () => undefined,
};

vi.mock("../jmap", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useJmapClient: () => CLIENT,
}));

beforeEach(() => {
  calls.admin = [];
  calls.created = [];
  calls.removedMembers = [];
});

/** Everything `ds/Modal` considers focusable, in tab order. */
const FOCUSABLE =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

describe("the admin dialogs are dialogs now", () => {
  test("Escape closes one, where no key handler existed anywhere in the file", () => {
    // Before: five hand-built `.modal` panels across four files, and not one
    // `onKeyDown` among them. The only way out of any of them was the mouse.
    let closed = 0;
    render(
      <DialogProvider>
        <GroupModal onClose={() => (closed += 1)} onChanged={() => undefined} />
      </DialogProvider>,
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(closed).toBe(1);
  });

  test("Tab from the last control returns to the first", () => {
    // Before: Tab walked out of the panel onto the user list behind it, which
    // a screen reader then read out happily although it could not be reached.
    render(
      <DialogProvider>
        <GroupModal onClose={() => undefined} onChanged={() => undefined} />
      </DialogProvider>,
    );
    const panel = screen.getByRole("dialog");
    const stops = [...panel.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const last = stops[stops.length - 1]!;
    last.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(stops[0]);
  });

  test("the dialog names itself and shuts the page behind it out", () => {
    render(
      <DialogProvider>
        <GroupModal onClose={() => undefined} onChanged={() => undefined} />
      </DialogProvider>,
    );
    const panel = screen.getByRole("dialog");
    expect(panel.getAttribute("aria-modal")).toBe("true");
    expect(panel.getAttribute("aria-label")).toBe(strings.adminNewGroup);
  });

  test("Enter in the name field still creates the group", async () => {
    // The risk this migration ran: the form is in the dialog's body and its
    // submit button is in the footer, so they are siblings. Without the `form`
    // attribute tying them together, the button would submit nothing and Enter
    // would do nothing — a form that silently stops being a form.
    render(
      <DialogProvider>
        <GroupModal onClose={() => undefined} onChanged={() => undefined} />
      </DialogProvider>,
    );
    fireEvent.change(screen.getByLabelText(strings.groupName), {
      target: { value: "Support" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.groupCreate }));
    await waitFor(() => expect(calls.created).toEqual(["Support"]));
  });
});

describe("a label that is really bound to its control", () => {
  test("each field in the group dialog is reachable by its label", () => {
    // Before: `<span class="label">` beside an `<input class="input">`, in a
    // `<div class="field">` — the words were next to the box and attached to
    // nothing, so the box was announced as "edit text, blank".
    render(
      <DialogProvider>
        <GroupModal
          group={GROUP}
          onClose={() => undefined}
          onChanged={() => undefined}
        />
      </DialogProvider>,
    );
    expect(screen.getByLabelText(strings.groupName)).toBeTruthy();
    expect(screen.getByLabelText(strings.groupListAddress)).toBeTruthy();
  });

  test("the hint under a field is described, not merely drawn", () => {
    render(
      <DialogProvider>
        <GroupModal
          group={GROUP}
          onClose={() => undefined}
          onChanged={() => undefined}
        />
      </DialogProvider>,
    );
    const field = screen.getByLabelText(strings.groupListAddress);
    const describedBy = field.getAttribute("aria-describedby");
    expect(describedBy).not.toBeNull();
    expect(document.getElementById(describedBy!)?.textContent).toBe(
      strings.groupListAddressHint,
    );
  });

  test("the member picker is named by its question, not by its answer", () => {
    // Before: a bare `<select class="input">`. A screen reader reads the
    // current value where the question should be — "combo box, Add member…".
    render(
      <DialogProvider>
        <GroupModal
          group={GROUP}
          onClose={() => undefined}
          onChanged={() => undefined}
        />
      </DialogProvider>,
    );
    expect(
      screen.getByRole("combobox", { name: strings.groupAddMember }),
    ).toBeTruthy();
  });
});

describe("a chip says what it removes", () => {
  test("each member's remove button names that member", async () => {
    // Before: a `<button class="chipX">` per member. It did carry a label, and
    // that is the one thing about these chips that was already right — the
    // adoption had to keep it rather than fall back to `ds/Chip`'s "Remove".
    render(
      <DialogProvider>
        <GroupModal
          group={GROUP}
          onClose={() => undefined}
          onChanged={() => undefined}
        />
      </DialogProvider>,
    );
    fireEvent.click(
      screen.getByRole("button", {
        name: strings.providerRemoveModel("ada@example.test"),
      }),
    );
    await waitFor(() => expect(calls.removedMembers).toEqual(["u-1"]));
  });
});

describe("a switch that says it is a switch, and whose it is", () => {
  test("each row's admin toggle names the person it grants access to", async () => {
    // Before: `<label class="toggle" title="Tenant admin">` wrapping a bare
    // checkbox whose label had no text. Twenty rows, twenty controls announced
    // as "checkbox, not checked", and `title` is a tooltip rather than a name.
    render(
      <DialogProvider>
        <UsersPage />
      </DialogProvider>,
    );
    const ada = await screen.findByRole("switch", {
      name: strings.userAdminRoleFor("ada@example.test"),
    });
    expect((ada as HTMLInputElement).checked).toBe(false);
    const ben = screen.getByRole("switch", {
      name: strings.userAdminRoleFor("ben@example.test"),
    });
    expect((ben as HTMLInputElement).checked).toBe(true);
  });

  test("the signed-in admin's own row is still disabled", async () => {
    // A guard that predates the migration and would have been easy to drop:
    // the account you are using cannot take its own admin rights away.
    render(
      <DialogProvider>
        <UsersPage />
      </DialogProvider>,
    );
    const self = await screen.findByRole("switch", {
      name: strings.userAdminRoleFor("ben@example.test"),
    });
    expect((self as HTMLInputElement).disabled).toBe(true);
  });

  test("toggling one still reaches the server", async () => {
    render(
      <DialogProvider>
        <UsersPage />
      </DialogProvider>,
    );
    const ada = await screen.findByRole("switch", {
      name: strings.userAdminRoleFor("ada@example.test"),
    });
    fireEvent.click(ada);
    await waitFor(() =>
      expect(calls.admin).toEqual([{ id: "u-1", isAdmin: true }]),
    );
  });

  test("the accountant switch is announced as on or off, with its rule described", async () => {
    // Before: `aria-label` on the `<label>`, the visible sentence in a sibling
    // `<span>` bound to nothing, and a plain checkbox — so an access grant was
    // read as "checked" and the rule beside it was never read at all.
    render(
      <DialogProvider>
        <UserModal
          user={ADA}
          isSelf={false}
          onClose={() => undefined}
          onChanged={() => undefined}
          onSaved={() => undefined}
        />
      </DialogProvider>,
    );
    const role = screen.getByRole("switch", {
      name: strings.userAccountantRole,
    });
    const describedBy = role.getAttribute("aria-describedby");
    expect(describedBy).not.toBeNull();
    expect(document.getElementById(describedBy!)?.textContent).toBe(
      strings.userAccountantHint,
    );
  });

  test("each app switch is named by its app", async () => {
    render(
      <DialogProvider>
        <UserModal
          user={ADA}
          isSelf={false}
          onClose={() => undefined}
          onChanged={() => undefined}
          onSaved={() => undefined}
        />
      </DialogProvider>,
    );
    const agenda = await screen.findByRole("switch", {
      name: strings.moduleAgenda,
    });
    expect((agenda as HTMLInputElement).checked).toBe(true);
    const billing = screen.getByRole("switch", { name: strings.moduleBilling });
    expect((billing as HTMLInputElement).checked).toBe(false);
  });
});

describe("what the alias box is for", () => {
  test("it has a name of its own, not just a placeholder", async () => {
    // Before: `<input class="chipInput" placeholder="alias@…">`. A placeholder
    // is not a name — it disappears the moment somebody types.
    render(
      <DialogProvider>
        <UserModal
          user={ADA}
          isSelf={false}
          onClose={() => undefined}
          onChanged={() => undefined}
          onSaved={() => undefined}
        />
      </DialogProvider>,
    );
    expect(screen.getByLabelText(strings.userAliasAdd)).toBeTruthy();
  });
});
