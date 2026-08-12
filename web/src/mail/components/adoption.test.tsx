// What mail's chrome gained by adopting `ds/` (D2.02).
//
// The look of a migration is a matter of opinion; these are the properties
// that were measurably absent before it — or that the adoption could quietly
// have broken — so they are the ones worth a build. Each test names what the
// hand-built version did instead.
import { cleanup, createEvent, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { strings } from "../../i18n";
import { DialogProvider } from "../../ds";
import { FlagDueControl } from "./FlagDueControl";
import { RecipientInput } from "./RecipientInput";
import { RichTextEditor } from "./RichTextEditor";
import { SnoozeMenu } from "./SnoozeMenu";

afterEach(cleanup);

/** Yesterday, as the ISO string the JMAP flag-due property carries. */
const YESTERDAY = new Date(Date.now() - 86_400_000).toISOString();

describe("the follow-up due-date chip", () => {
  test("it says it opens a menu, and whether the menu is open", () => {
    // Before: a hand-drawn pill with a `title` and nothing else, so a screen
    // reader announced a button that did something unspecified when pressed.
    render(<FlagDueControl due={null} onSet={() => {}} />);
    const chip = screen.getByRole("button", { name: strings.flagDueAdd });
    expect(chip.getAttribute("aria-haspopup")).toBe("menu");
    expect(chip.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(chip);
    expect(chip.getAttribute("aria-expanded")).toBe("true");
  });

  test("overdue is said in words, not only in red", () => {
    // `Chip`'s `danger` tone carries the same meaning as the colour did, and
    // the tone is never the only signal: the label says it too.
    render(<FlagDueControl due={YESTERDAY} onSet={() => {}} />);
    const chip = screen.getByRole("button", { name: /Overdue/ });
    expect(chip.textContent).toContain("Overdue");
  });

  test("picking a date reports the end of that day, not its midnight", () => {
    // The whole point of the control, and the part an adoption could break by
    // handing `Input` an event shape it did not expect.
    let picked: number | null = -1;
    render(<FlagDueControl due={null} onSet={(v) => (picked = v)} />);
    fireEvent.click(screen.getByRole("button", { name: strings.flagDueAdd }));
    const date = screen.getByLabelText(strings.flagDuePick);
    fireEvent.change(date, { target: { value: "2026-03-04" } });
    expect(picked).toBe(Math.floor(new Date(2026, 2, 4, 23, 59, 0, 0).getTime() / 1000));
  });
});

describe("the snooze trigger", () => {
  test("adopting the shared icon button did not turn an action into a toggle", () => {
    // `IconButton` used to set `aria-pressed` on everything it rendered, which
    // would have announced this menu trigger as an unpressed toggle button.
    render(<SnoozeMenu onPick={() => {}} />);
    const trigger = screen.getByRole("button", { name: strings.snooze });
    expect(trigger.hasAttribute("aria-pressed")).toBe(false);
    expect(trigger.getAttribute("aria-haspopup")).toBe("menu");
  });

  test("it still opens the presets, and picking one reports a wake time", () => {
    let woke = 0;
    render(<SnoozeMenu onPick={(at) => (woke = at)} compact />);
    fireEvent.click(screen.getByRole("button", { name: strings.snooze }));
    const menu = screen.getByRole("menu");
    const first = menu.querySelectorAll("button")[0];
    expect(first).toBeDefined();
    fireEvent.click(first!);
    expect(woke).toBeGreaterThan(Math.floor(Date.now() / 1000));
  });
});

describe("the recipient field", () => {
  test("each chip's remove button names the recipient it removes", () => {
    // A row of buttons all called "Remove" is useless read aloud, which is why
    // `Chip.removeLabel` exists rather than a default label.
    render(
      <RecipientInput
        label={strings.composeTo}
        value={[{ name: "Ada", email: "ada@example.test" }]}
        onChange={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: strings.removeRecipient("Ada") })).toBeDefined();
  });

  test("removing a chip removes that recipient and keeps the rest", () => {
    let next: { name: string | null; email: string }[] = [];
    render(
      <RecipientInput
        label={strings.composeTo}
        value={[
          { name: null, email: "a@example.test" },
          { name: null, email: "b@example.test" },
        ]}
        onChange={(v) => (next = v)}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", { name: strings.removeRecipient("a@example.test") }),
    );
    expect(next.map((a) => a.email)).toEqual(["b@example.test"]);
  });
});

describe("the compose formatting bar", () => {
  function editor() {
    return render(
      <DialogProvider>
        <RichTextEditor initialHtml="" onChange={() => {}} placeholder="Body" />
      </DialogProvider>,
    );
  }

  test("it is a named group, and no longer claims a keyboard model it lacks", () => {
    // Before: `role="toolbar"` with no arrow keys, Home or End — a promise
    // made in a role is still a promise. The row holds two selects and two
    // colour pickers, so it is a group whose controls keep their tab stops.
    editor();
    expect(screen.getByRole("group", { name: strings.formatting })).toBeDefined();
    expect(screen.queryByRole("toolbar")).toBeNull();
  });

  test("a formatting button is not announced as an unpressed toggle", () => {
    // Twelve icon buttons on one row: "Bold, not pressed" over bold text is
    // worse than saying nothing, and nothing here tracks the caret's state.
    editor();
    expect(
      screen.getByRole("button", { name: strings.bold }).hasAttribute("aria-pressed"),
    ).toBe(false);
  });

  test("pressing a tool does not steal the selection from the body", () => {
    // The mousedown default is what would blur the contentEditable and lose
    // the caret; it has to survive the move to `IconButton`'s prop spread.
    editor();
    const bold = screen.getByRole("button", { name: strings.bold });
    const event = createEvent.mouseDown(bold);
    fireEvent(bold, event);
    expect(event.defaultPrevented).toBe(true);
  });

  test("both dropdowns still say what they choose", () => {
    // A select with no name is announced as its own current value — "combo
    // box, Sans Serif" reads as though the font were the question.
    editor();
    expect(screen.getByRole("combobox", { name: strings.fontFamily })).toBeDefined();
    expect(screen.getByRole("combobox", { name: strings.fontSize })).toBeDefined();
  });
});
