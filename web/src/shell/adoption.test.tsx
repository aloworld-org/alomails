// What the shell gained by adopting `ds/` (D2.03).
//
// The look of a migration is a matter of opinion; these are the properties
// that were measurably absent before it — or that the adoption could quietly
// have broken — so they are the ones worth a build. Each test names what the
// hand-built version did instead.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { Inbox } from "lucide-react";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import type { AgentActionDto, Delegate, MailFilterRule } from "../jmap";
import { AgentActionCard } from "./AgentActionCard";
import { ComingSoon } from "./ComingSoon";
import { FiltersSection } from "./FiltersSection";
import { SettingsModal } from "./SettingsModal";
import { SharingSection } from "./SharingSection";

afterEach(cleanup);

/** One saved rule, as `filters()` returns it. */
const RULE: MailFilterRule = {
  id: "rule-1",
  name: "Invoices",
  match: "all",
  conditions: [{ field: "from", op: "contains", value: "billing@" }],
  actions: [{ type: "fileInto", mailbox: "Finance" }],
  enabled: true,
};

/** One grant, as `myDelegates()` returns it. */
const GRANT: Delegate = {
  id: "grant-1",
  email: "ben@example.test",
  canWrite: false,
  sendMode: "none",
  folders: [],
};

const state = {
  rules: [RULE] as MailFilterRule[],
  delegates: [GRANT] as Delegate[],
};

vi.mock("../jmap", () => ({
  useJmapClient: () => ({
    mailSettings: async () => ({
      signature: "",
      orgFooter: "",
      outOfOffice: {
        enabled: false,
        subject: "",
        message: "",
        from: null,
        to: null,
      },
    }),
    filters: async () => state.rules,
    mailboxes: async () => [
      { id: "mb-1", name: "Finance", role: null },
      { id: "mb-2", name: "Archive", role: null },
    ],
    saveFilters: async (next: MailFilterRule[]) => next,
    myDelegates: async () => state.delegates,
    ownAccountId: async () => "acct-1",
    mailboxesFor: async () => [
      { id: "mb-1", name: "Finance", role: null },
      { id: "mb-2", name: "Archive", role: null },
    ],
    shareMyMailbox: async () => undefined,
    unshareMyMailbox: async () => undefined,
  }),
}));

beforeEach(() => {
  state.rules = [RULE];
  state.delegates = [GRANT];
});

describe("Settings is a dialog rather than a div that looks like one", () => {
  /** Settings mounts a rich-text editor, which asks for dialogs. */
  function mount(onClose = () => undefined) {
    return render(
      <DialogProvider>
        <SettingsModal isAdmin={false} onClose={onClose} />
      </DialogProvider>,
    );
  }

  test("Escape closes it", async () => {
    // Before: no key handler anywhere in the file. The only way out of
    // Settings was the mouse — two buttons and the backdrop, all of them
    // pointer targets.
    let closed = 0;
    mount(() => {
      closed += 1;
    });
    await screen.findByRole("dialog");
    fireEvent.keyDown(document, { key: "Escape" });
    expect(closed).toBe(1);
  });

  test("it names itself, and says the page behind it is out of reach", () => {
    // `aria-modal` was there before; the trap that makes it true was not.
    mount();
    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(dialog.getAttribute("aria-label")).toBe(strings.settingsTitle);
  });

  test("Tab from the last control returns to the first", async () => {
    // Before: Tab walked straight out of the panel onto the page underneath,
    // where a screen reader read out a mailbox the user could no longer see.
    mount();
    await screen.findByRole("dialog");
    const dialog = screen.getByRole("dialog");
    const focusable = [
      ...dialog.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    ];
    const last = focusable[focusable.length - 1]!;
    last.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(focusable[0]);
  });

  test("the out-of-office switch is announced as a switch, and named", async () => {
    // Before: a bare `<input type="checkbox">` inside a `<span>`, with the
    // words in a sibling span bound to nothing — announced as "checkbox, not
    // checked", and the hint under it was on screen and never read.
    mount();
    const sw = await screen.findByRole("switch", {
      name: strings.settingsOooToggle,
    });
    expect(
      sw.getAttribute("aria-checked") ?? String(sw.hasAttribute("checked")),
    ).toBeTruthy();
    const describedBy = sw.getAttribute("aria-describedby");
    expect(describedBy).toBeTruthy();
    expect(document.getElementById(describedBy!)?.textContent).toBe(
      strings.settingsOutOfOfficeHint,
    );
  });

  test("turning it on reveals a subject field that has a name", async () => {
    // Before: an `<input>` with a placeholder and nothing else. A placeholder
    // disappears the moment you type into it.
    mount();
    const sw = await screen.findByRole("switch", {
      name: strings.settingsOooToggle,
    });
    fireEvent.click(sw);
    expect(
      screen.getByLabelText(strings.settingsOooSubjectPlaceholder),
    ).toBeTruthy();
  });
});

describe("the filter rule editor", () => {
  test("every rule's on/off box says which rule it belongs to", async () => {
    // Before: `<label><input type="checkbox"/></label>` with no text at all,
    // once per rule — a column of boxes announced as "checkbox, checked".
    render(<FiltersSection />);
    expect(
      await screen.findByRole("checkbox", {
        name: strings.filterRuleEnabled("Invoices"),
      }),
    ).toBeTruthy();
  });

  test("a rule with no name is still named by what it does", async () => {
    // The list draws the summary for an unnamed rule; the control has to say
    // the same thing rather than fall back to nothing.
    state.rules = [{ ...RULE, name: "  " }];
    render(<FiltersSection />);
    const boxes = await screen.findAllByRole("checkbox");
    // Read but not drawn: the name lives in the wrapping label, hidden.
    const name = boxes[0]!.closest("label")?.textContent ?? "";
    expect(name).not.toBe("");
    expect(name).toContain("billing@");
  });

  test("each condition's two dropdowns say which condition they belong to", async () => {
    // Before: two `<select>`s with no label, no `aria-label` and no wrapping
    // `<label>` — the pair `Select` was written about. A screen reader read
    // the current value where the question should be: "combo box, From".
    render(<FiltersSection />);
    fireEvent.click(await screen.findByText("Invoices"));
    expect(screen.getByLabelText(strings.filterConditionField(1))).toBeTruthy();
    expect(screen.getByLabelText(strings.filterConditionOp(1))).toBeTruthy();
    expect(screen.getByLabelText(strings.filterConditionValue(1))).toBeTruthy();
  });

  test("a second condition is numbered as the second, not as another first", async () => {
    render(<FiltersSection />);
    fireEvent.click(await screen.findByText("Invoices"));
    fireEvent.click(screen.getByText(strings.filterAddCondition));
    expect(screen.getByLabelText(strings.filterConditionField(2))).toBeTruthy();
    expect(
      screen.getByRole("button", { name: strings.filterRemoveConditionAt(2) }),
    ).toBeTruthy();
  });

  test("the folder dropdown is not inside the checkbox's label", async () => {
    // Before: `<label>` wrapped the box, the words and the select. A label
    // binds to the first control it contains, so the select was unnamed and
    // clicking its words toggled the checkbox next to it.
    render(<FiltersSection />);
    fireEvent.click(await screen.findByText("Invoices"));
    const folder = screen.getByLabelText(strings.filterFolderLabel);
    expect(folder.closest("label")).toBeNull();
    const box = screen.getByRole("checkbox", {
      name: strings.filterActionFileInto,
    });
    expect(box.closest("label")?.contains(folder)).toBe(false);
  });
});

describe("mailbox sharing", () => {
  test("the buttons on a row name the person that row is about", async () => {
    // Before: `aria-label="Remove access"` on every row. In a list of five
    // colleagues that is the same sentence five times, and nothing in it says
    // whose access is about to go.
    render(<SharingSection />);
    expect(
      await screen.findByRole("button", {
        name: strings.delegateRemoveFor(GRANT.email),
      }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", {
        name: strings.delegateFoldersFor(GRANT.email),
      }),
    ).toBeTruthy();
  });

  test("the folder button says whether the folder list is open", async () => {
    render(<SharingSection />);
    const button = await screen.findByRole("button", {
      name: strings.delegateFoldersFor(GRANT.email),
    });
    expect(button.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(button);
    expect(button.getAttribute("aria-expanded")).toBe("true");
  });

  test("each folder box carries its own folder's name", async () => {
    // Before: the name sat beside the box as a bare text node inside the
    // label, which worked — but the same class was also put on a `<span>`
    // elsewhere in the shell, where it bound nothing. `Checkbox` owns the
    // label, so the wrong version is not writable.
    render(<SharingSection />);
    fireEvent.click(
      await screen.findByRole("checkbox", {
        name: strings.delegateLimitFolders,
      }),
    );
    await waitFor(() => {
      expect(screen.getByRole("checkbox", { name: "Finance" })).toBeTruthy();
    });
    expect(screen.getByRole("checkbox", { name: "Archive" })).toBeTruthy();
  });

  test("the whole-mailbox add form keeps its Add button working", async () => {
    // `.add` was a bare `<button type="submit">`; `ds/Button` defaults to
    // "button", so the swap is checked below. This one guards the other half:
    // the button is still refused while the address is empty.
    render(<SharingSection />);
    const add = await screen.findByRole("button", { name: strings.sharingAdd });
    expect((add as HTMLButtonElement).disabled).toBe(true);
  });

  test("adding a colleague still submits the form", async () => {
    // The adoption swapped a raw `<button type="submit">` for `ds/Button`,
    // which defaults `type` to "button" — a default that would have made the
    // Add button do nothing at all.
    render(<SharingSection />);
    const add = await screen.findByRole("button", { name: strings.sharingAdd });
    expect(add.getAttribute("type")).toBe("submit");
  });
});

describe("the surfaces that only changed hands", () => {
  test("the placeholder's icon plate is decoration, not something to read", () => {
    // It used to be `.badge`, which is a `ds/Badge`'s name and not its job: a
    // badge states a fact in words. This is a 56px plate holding the module's
    // own glyph, and a screen reader has nothing to say about it.
    const { container } = render(
      <ComingSoon title="Agenda" body="Not yet." Icon={Inbox} />,
    );
    const plate = container.querySelector("svg")?.parentElement;
    expect(plate?.getAttribute("aria-hidden")).toBe("true");
  });

  test("a running agent action refuses both decisions", () => {
    // `.approve`/`.discard` became `ds/Button`, which brings its own disabled
    // rendering. What must not change is that neither can be pressed twice
    // while the first one is still executing.
    const action: AgentActionDto = {
      tool: "archive_email",
      args: { subject: "Invoice 42" },
      say: "Archive it.",
    };
    let approved = 0;
    render(
      <AgentActionCard
        action={action}
        running
        onApprove={() => {
          approved += 1;
        }}
        onDiscard={() => undefined}
      />,
    );
    const discard = screen.getByRole("button", { name: strings.agentDiscard });
    expect((discard as HTMLButtonElement).disabled).toBe(true);
    const buttons = screen.getAllByRole("button") as HTMLButtonElement[];
    for (const button of buttons) fireEvent.click(button);
    expect(approved).toBe(0);
  });
});
