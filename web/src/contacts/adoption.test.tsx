// What the address book gained by adopting `ds/` (D2.06).
//
// The look of a migration is a matter of opinion; these are the properties
// that were measurably absent before it — or that the adoption could quietly
// have broken — so they are the ones worth a build. Each test names what the
// hand-built version did instead.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import type { Contact } from "../jmap";
import { ContactsModal } from "./ContactsModal";

afterEach(cleanup);

const ADA: Contact = {
  id: "c-1",
  name: "Ada Lovelace",
  firstName: "Ada",
  lastName: "Lovelace",
  emails: [{ kind: "work", value: "ada@example.test" }],
  phones: [],
  organization: "Analytical Engines",
  jobTitle: null,
  notes: null,
};

vi.mock("../jmap", () => ({
  useJmapClient: () => ({
    contacts: async () => [ADA],
    createContact: async () => "c-2",
    updateContact: async () => undefined,
    deleteContact: async () => undefined,
    importContacts: async () => ({ imported: 0, skipped: 0 }),
    exportContacts: async () => "",
  }),
}));

// The contact in focus now shows its agent (A8.4), which reads the agent
// directory and links into the chat room — so the dialog needs the workspace's
// authorized fetch and a router around it, as it has in the app.
vi.mock("../auth", () => ({
  useAuth: () => ({
    authorizedFetch: async () =>
      new Response(JSON.stringify({ agents: [] }), {
        headers: { "content-type": "application/json" },
      }),
  }),
}));

function open(onClose = () => {}) {
  return render(
    <MemoryRouter>
      <DialogProvider>
        <ContactsModal onClose={onClose} />
      </DialogProvider>
    </MemoryRouter>,
  );
}

/** Opens the dialog and selects Ada, which is what draws the detail form. */
async function openContact() {
  open();
  fireEvent.click(await screen.findByText("Ada Lovelace"));
}

describe("the address book is a dialog", () => {
  test("Escape still closes it, now from the component", () => {
    // Before: a `document` keydown listener written in this file — one of only
    // two in the codebase that handled a key at all. It is `ds/Modal`'s now,
    // and this test is here because that is exactly the kind of behaviour a
    // migration loses without anybody noticing.
    const onClose = vi.fn();
    open(onClose);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  test("Tab cannot leave it", async () => {
    // Before: nothing. Tab walked out of the panel onto the page behind it,
    // where a screen reader would read out the mail list the scrim was
    // covering.
    open();
    // The last control in the panel is the last row of the list; the first is
    // the Import button in the header.
    const row = (await screen.findByText("Ada Lovelace")).closest("button")!;
    const first = screen.getByRole("button", { name: strings.contactsImport });
    row.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(row);
  });

  test("the opening focus lands on a control, not on the import picker", () => {
    // The file input is `hidden` — it exists to be clicked by the Import
    // button. It matches the focusable selector, and `focus()` on it is a
    // silent no-op, so the caret would have stayed on the page behind. This is
    // the defect this migration found in `ds/Modal` and fixed there.
    open();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: strings.contactsImport }),
    );
  });
});

describe("the contact form is reachable by its labels", () => {
  test("every text field is found by its label", async () => {
    // Before: `<label class="field"><span>First name</span><input>` — a
    // wrapping label, which does bind, but only because all seven remembered
    // to wrap. `ds/Field` binds by id whether they remember or not.
    await openContact();
    for (const label of [
      strings.contactFirstName,
      strings.contactLastName,
      strings.contactDisplayName,
      strings.contactOrganization,
      strings.contactJobTitle,
    ]) {
      expect(screen.getByLabelText(label).tagName).toBe("INPUT");
    }
    // The notes box is the one control here the design system cannot yet
    // supply — there is no multi-line primitive — so it is bound by hand, and
    // that hand-wiring is what this line holds.
    expect(screen.getByLabelText(strings.contactNotes).tagName).toBe(
      "TEXTAREA",
    );
  });

  test("a row's kind picker and remove button are named after the row", async () => {
    // Before: every remove button was "Remove" and every kind picker was
    // announced "Other" — one contact with a work email, a home email and two
    // phones gave a screen reader four identical commands and four identical
    // combo boxes.
    await openContact();
    expect(
      screen.getByRole("button", {
        name: strings.contactRemoveFieldNamed("ada@example.test"),
      }),
    ).toBeTruthy();
    expect(
      screen.getByLabelText(strings.contactKindLabel("ada@example.test")),
    ).toBeTruthy();
    // A blank row has no value to be named after, so it falls back to the
    // field's own name rather than being nameless.
    fireEvent.click(screen.getByText(strings.contactAddPhone));
    expect(
      screen.getByRole("button", {
        name: strings.contactRemoveFieldNamed(strings.contactPhone),
      }),
    ).toBeTruthy();
  });

  test("adding and removing a row still edits the right one", async () => {
    // The behaviour the migration could most easily have broken: the rows are
    // indexed, and the controls that were rewritten are the ones carrying the
    // index.
    await openContact();
    fireEvent.click(screen.getByText(strings.contactAddEmail));
    const blank = screen.getByRole("button", {
      name: strings.contactRemoveFieldNamed(strings.contactEmail),
    });
    fireEvent.click(blank);
    // The stored email survives; only the row that was added has gone.
    expect(
      screen.getByRole("button", {
        name: strings.contactRemoveFieldNamed("ada@example.test"),
      }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("button", {
        name: strings.contactRemoveFieldNamed(strings.contactEmail),
      }),
    ).toBeNull();
  });
});
