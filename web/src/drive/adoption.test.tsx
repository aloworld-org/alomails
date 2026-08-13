// What Drive gained by adopting `ds/` (D2.04).
//
// The look of a migration is a matter of opinion; these are the properties
// that were measurably absent before it — or that the adoption could quietly
// have broken. Each test names what the hand-built version did instead.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import type {
  BaseFieldDto,
  BaseTableDto,
  DriveVersionDto,
  SpaceDto,
  SpaceDetailDto,
} from "../jmap";
import { Cell, CellDisplay, chipColor } from "./BaseCell";
import { DestinationDialog, MembersDialog, VersionsDialog } from "./dialogs";

afterEach(cleanup);

const SPACE: SpaceDto = {
  id: "sp-1",
  name: "Marketing",
  archived: false,
  myRole: "manager",
  createdAt: "2026-01-01T00:00:00Z",
};

const DETAIL: SpaceDetailDto = {
  space: SPACE,
  members: [
    { userId: "u-1", email: "ada@example.test", role: "manager", addedAt: "2026-01-01T00:00:00Z" },
    { userId: "u-2", email: "ben@example.test", role: "viewer", addedAt: "2026-01-02T00:00:00Z" },
  ],
  modules: [],
};

const VERSIONS: DriveVersionDto[] = [
  { versionNo: 2, blobId: "b-2", size: 2048, createdBy: "u-1", createdAt: "2026-02-02T00:00:00Z" },
  { versionNo: 1, blobId: "b-1", size: 1024, createdBy: "u-1", createdAt: "2026-02-01T00:00:00Z" },
];

const calls = {
  added: [] as { email: string; role: string }[],
  versionsFail: false,
};

// One client object for the whole file, not a fresh one per render: both
// dialogs list `client` in an effect's dependencies, so a new identity every
// render would re-fetch forever and the test would be measuring a render loop
// rather than the component.
const CLIENT = {
  spaceDetail: async () => DETAIL,
  addSpaceMember: async (_space: string, email: string, role: string) => {
    calls.added.push({ email, role });
  },
  removeSpaceMember: async () => undefined,
  driveVersions: async () => {
    if (calls.versionsFail) throw new Error("nope");
    return VERSIONS;
  },
  driveRestoreVersion: async () => undefined,
};

vi.mock("../jmap", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useJmapClient: () => CLIENT,
}));

beforeEach(() => {
  calls.added = [];
  calls.versionsFail = false;
});

/** Everything `ds/Modal` considers focusable, in tab order. */
const FOCUSABLE =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

describe("the three Drive dialogs are dialogs, not divs that look like one", () => {
  test("each one names itself and says the page behind it is out of reach", async () => {
    // Before: a `div.scrim` holding a `div.dialog` with an `<h2>` in it. No
    // role, no `aria-modal`, no accessible name — a screen reader announced
    // nothing at all when it opened, and the file list behind it stayed in
    // the reading order.
    render(
      <DestinationDialog spaces={[SPACE]} mode="move" onPick={() => undefined} onClose={() => undefined} />,
    );
    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(dialog.getAttribute("aria-label")).toBe(strings.driveMoveTo);
  });

  test("Escape closes it", () => {
    // Before: no key handler in the file. The only way out of "Move to…" was
    // the mouse — the close button or the backdrop, both pointer targets.
    let closed = 0;
    render(
      <DestinationDialog
        spaces={[SPACE]}
        mode="copy"
        onPick={() => undefined}
        onClose={() => {
          closed += 1;
        }}
      />,
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(closed).toBe(1);
  });

  test("a click on the backdrop still dismisses", () => {
    // The one behaviour the hand-built version did have — `onMouseDown` on the
    // scrim with `stopPropagation` on the panel. Adoption had to keep it.
    let closed = 0;
    const { container } = render(
      <DestinationDialog
        spaces={[SPACE]}
        mode="move"
        onPick={() => undefined}
        onClose={() => {
          closed += 1;
        }}
      />,
    );
    const overlay = container.firstElementChild!;
    fireEvent.mouseDown(overlay);
    expect(closed).toBe(1);
    // ...and a press that lands inside the panel does not, which is what stops
    // a text selection dragged past the edge from throwing the dialog away.
    fireEvent.mouseDown(screen.getByRole("dialog"));
    expect(closed).toBe(1);
  });

  test("Tab from the last control returns to the first", async () => {
    // Before: Tab walked straight out of the panel onto the file list, where a
    // screen reader read out files the user could no longer see or click.
    render(
      <DialogProvider>
        <MembersDialog space={SPACE} onClose={() => undefined} />
      </DialogProvider>,
    );
    await screen.findByText("ada@example.test");
    const dialog = screen.getByRole("dialog");
    const focusable = [...dialog.querySelectorAll<HTMLElement>(FOCUSABLE)];
    expect(focusable.length).toBeGreaterThan(1);
    const first = focusable[0]!;
    const last = focusable[focusable.length - 1]!;
    last.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
  });

  test("closing gives focus back to whatever opened it", () => {
    // Before: dismissing dropped the caret at the top of the document, so a
    // keyboard user had to travel back down to the file they were working on.
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const view = render(<VersionsDialog nodeId="n-1" onChanged={() => undefined} onClose={() => undefined} />);
    expect(document.activeElement).not.toBe(opener);
    view.unmount();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });
});

describe("the members dialog's controls say what they are", () => {
  test("the role select is announced by its question, not by its answer", async () => {
    // Before: a bare `<select class=addRole>` with no label of any kind. A
    // screen reader announced "combo box, Editor" — the current value read as
    // though it were the question. `ds/Select` shouts about this in dev, so a
    // missing name is now a failure at the console as well as here.
    const errors: unknown[] = [];
    const spy = vi.spyOn(console, "error").mockImplementation((...args) => {
      errors.push(args);
    });
    render(
      <DialogProvider>
        <MembersDialog space={SPACE} onClose={() => undefined} />
      </DialogProvider>,
    );
    await screen.findByText("ada@example.test");
    expect(screen.getByLabelText(strings.driveMemberRoleLabel).tagName).toBe("SELECT");
    expect(screen.getByLabelText(strings.driveAddMemberLabel).tagName).toBe("INPUT");
    expect(errors).toEqual([]);
    spy.mockRestore();
  });

  test("each Remove button names the person it removes", async () => {
    // Before: every row's button was `aria-label="Remove"`. Read aloud, a
    // membership list was "Remove, Remove, Remove" with no way to tell which
    // one was about to take away your colleague's access.
    render(
      <DialogProvider>
        <MembersDialog space={SPACE} onClose={() => undefined} />
      </DialogProvider>,
    );
    await screen.findByText("ada@example.test");
    expect(screen.getByLabelText(strings.driveRemoveMemberFor("ben@example.test"))).toBeTruthy();
    expect(screen.queryByLabelText(strings.driveRemoveMember)).toBeNull();
  });

  test("Enter in the email field still adds the member", async () => {
    // The adoption swapped a native `<input>` for `ds/Input`; if the component
    // had not passed `onKeyDown` through, the keyboard path would have gone
    // silently dead while the Add button kept working.
    render(
      <DialogProvider>
        <MembersDialog space={SPACE} onClose={() => undefined} />
      </DialogProvider>,
    );
    await screen.findByText("ada@example.test");
    const email = screen.getByLabelText(strings.driveAddMemberLabel);
    fireEvent.change(email, { target: { value: "cleo@example.test" } });
    fireEvent.keyDown(email, { key: "Enter" });
    await waitFor(() => expect(calls.added).toEqual([{ email: "cleo@example.test", role: "editor" }]));
  });

  test("the Add button is a button, not a submit", async () => {
    // `ds/Button` defaults `type` to "button". If it did not, a dialog that
    // ever gains a `<form>` would submit and reload the page on Add.
    render(
      <DialogProvider>
        <MembersDialog space={SPACE} onClose={() => undefined} />
      </DialogProvider>,
    );
    await screen.findByText("ada@example.test");
    expect(screen.getByRole("button", { name: strings.driveAdd }).getAttribute("type")).toBe("button");
  });
});

describe("a failed load still announces itself", () => {
  test("the version list's error keeps its live region", async () => {
    // Moving the body inside `ds/Modal` must not lose the `role="alert"` that
    // makes a failure audible rather than merely visible.
    calls.versionsFail = true;
    render(<VersionsDialog nodeId="n-1" onChanged={() => undefined} onClose={() => undefined} />);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain(strings.driveRetry);
  });
});

const FIELD = (type: BaseFieldDto["type"], name: string): BaseFieldDto => ({
  id: `f-${type}`,
  name,
  type,
  options: {},
});

const TABLE: BaseTableDto = {
  id: "t-1",
  name: "People",
  fields: [FIELD("text", "Name")],
  records: [{ id: "r-1", cells: { "f-text": "Ada" } }],
  views: [],
};

describe("a Base cell is an input with a name", () => {
  test("every cell editor is named by its column", () => {
    // Before: a grid of `<input class=input>` with nothing but a placeholder on
    // one of them. Read aloud, a row of eight cells was eight anonymous edit
    // fields, and the column heading — the only thing that said what the value
    // meant — was never announced with them.
    render(<Cell field={FIELD("number", "Budget")} value={12} tables={[]} onCommit={() => undefined} />);
    expect(screen.getByLabelText("Budget").getAttribute("type")).toBe("number");
  });

  test("it still commits on blur", () => {
    // The cell editor moved from a local class to `ds/Input`'s `variant="cell"`.
    // If the component swallowed `onBlur`, every edit in every Base would be
    // discarded the moment the user left the cell — silently.
    const committed: unknown[] = [];
    render(
      <Cell
        field={FIELD("text", "Name")}
        value="Ada"
        tables={[]}
        onCommit={(v) => committed.push(v)}
      />,
    );
    const input = screen.getByLabelText("Name");
    fireEvent.blur(input, { target: { value: "Ada L." } });
    expect(committed).toEqual(["Ada L."]);
  });
});

describe("a Base chip keeps the colour that tells its values apart", () => {
  test("the choice's colour survives the move to ds/Chip", () => {
    // The local `.chip` mixed a per-value colour from a `--cc` custom property.
    // `ds/Chip` has named tones, which cannot describe choices a user invents,
    // so it gained `color` — and this is the test that the colour still lands.
    const { container } = render(
      <CellDisplay field={FIELD("select", "Stage")} value="Won" tables={[]} />,
    );
    const chip = container.querySelector("span")!;
    expect(chip.style.getPropertyValue("--chip-color")).toBe(chipColor("Won"));
    // Colour is never the only signal: the chip says which choice it is.
    expect(chip.textContent).toBe("Won");
  });

  test("a link chip carries no colour, because a record id has no meaning to give one", () => {
    const { container } = render(
      <CellDisplay
        field={{ ...FIELD("link", "Owner"), options: { linkTableId: "t-1" } }}
        value={["r-1"]}
        tables={[TABLE]}
      />,
    );
    // The chips themselves, not the row that wraps them — which carries the
    // same text once there is only one chip in it.
    const chips = [...container.querySelectorAll("span")].filter(
      (el) => el.textContent === "Ada" && el.childElementCount === 0,
    );
    expect(chips).toHaveLength(1);
    expect(chips[0]!.style.getPropertyValue("--chip-color")).toBe("");
  });
});
