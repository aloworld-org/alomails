// The keyboard contract of the sites dialogs (S2.16b).
//
// Every one of them announced `role="dialog" aria-modal="true"` and none of
// them behaved like one: focus stayed behind the scrim, Tab walked out onto
// the covered page, Escape only worked once focus was already inside, and
// closing dropped the caret at the top of the document. These tests fail
// against that code and pass against `useDialogKeyboard`.
//
// `DialogFrame` is the hand-written chrome every dialog on the surface
// inherits — twelve of them: new site, new page, theme, SEO, publish, catalog
// item, domain purchase, handoff, and the section prop forms. The section
// library now uses the shared design-system Modal directly; its focused
// category and placement contract is asserted in `SectionPalette.test.tsx`.
import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { Sparkles } from "lucide-react";

import { strings } from "../i18n";
import { DialogFrame, Field } from "./parts";

afterEach(cleanup);

/** A dialog opened from a button, the way every screen in the module opens
 *  one — so "focus went in" and "focus came back" are about a real opener and
 *  not about a component rendered already-open in a bare document. */
function Harness({
  autoFocusField = false,
  onClose,
}: {
  autoFocusField?: boolean;
  onClose?: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button type="button" onClick={() => setOpen(true)}>
        open
      </button>
      <button type="button">behind</button>
      {open && (
        <DialogFrame
          Icon={Sparkles}
          title="Test dialog"
          subtitle="A dialog to press keys at"
          error={null}
          busy={false}
          canSubmit={true}
          submitLabel="Save"
          onClose={() => {
            setOpen(false);
            onClose?.();
          }}
          onSubmit={() => {}}
        >
          <Field label="Name">
            <input autoFocus={autoFocusField} defaultValue="" />
          </Field>
        </DialogFrame>
      )}
    </div>
  );
}

function openDialog() {
  const trigger = screen.getByRole("button", { name: "open" });
  trigger.focus();
  fireEvent.click(trigger);
  return trigger;
}

describe("a sites dialog can be used without a mouse", () => {
  test("opening moves focus off the page behind and into the panel", () => {
    render(<Harness />);
    openDialog();
    const dialog = screen.getByRole("dialog");
    expect(dialog.contains(document.activeElement)).toBe(true);
    // The close button is the first control in the chrome, so that is where a
    // dialog with no autofocused field starts.
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: strings.close }),
    );
  });

  test("a field that asks for focus keeps it", () => {
    render(<Harness autoFocusField={true} />);
    openDialog();
    expect(document.activeElement).toBe(screen.getByLabelText("Name"));
  });

  test("Escape closes it while focus is still on the button that opened it", () => {
    const closed = vi.fn();
    render(<Harness onClose={closed} />);
    const trigger = openDialog();
    // The state the old panel-level handler could not see: focus put back
    // outside the dialog by anything at all.
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "Escape" });
    expect(closed).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("closing gives focus back to the control that opened it", () => {
    render(<Harness />);
    const trigger = openDialog();
    fireEvent.click(screen.getByRole("button", { name: strings.close }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  test("Tab cannot walk out of the dialog onto the covered page", () => {
    render(<Harness />);
    openDialog();
    const dialog = screen.getByRole("dialog");
    const stops = [
      ...dialog.querySelectorAll<HTMLElement>("button, input"),
    ].filter((el) => !el.hasAttribute("disabled"));
    expect(stops.length).toBeGreaterThan(2);
    const first = stops[0]!;
    const last = stops[stops.length - 1]!;

    last.focus();
    fireEvent.keyDown(last, { key: "Tab" });
    expect(document.activeElement).toBe(first);

    first.focus();
    fireEvent.keyDown(first, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
  });

  test("Tab from outside the panel is pulled back in, not left behind it", () => {
    render(<Harness />);
    openDialog();
    const behind = screen.getByRole("button", { name: "behind" });
    behind.focus();
    fireEvent.keyDown(behind, { key: "Tab" });
    expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(
      true,
    );
  });

  test("a re-render of the screen does not yank focus back to the first control", () => {
    render(<Harness />);
    openDialog();
    const field = screen.getByLabelText("Name");
    field.focus();
    // The onClose the dialog was given is a fresh closure on every render;
    // an effect that depended on it would re-run here and refocus the close
    // button under the user's fingers.
    fireEvent.change(field, { target: { value: "typing" } });
    expect(document.activeElement).toBe(field);
  });
});
