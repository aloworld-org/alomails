// What sixteen hand-built modals did not do.
//
// The look of a dialog is obvious when you open one; the behaviour is
// invisible until somebody without a mouse tries to use it. These are the
// four properties that were missing across the codebase, so they are the four
// worth holding a build hostage to.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { Modal } from "./Modal";

afterEach(cleanup);

function open(onClose = () => {}) {
  return render(
    <Modal
      title="Invite a colleague"
      onClose={onClose}
      footer={<button>Send</button>}
    >
      <input aria-label="email" />
      <button>Middle</button>
    </Modal>,
  );
}

describe("the modal behaves for somebody without a mouse", () => {
  test("Escape closes it", () => {
    const onClose = vi.fn();
    open(onClose);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  test("focus lands inside it, not on the page behind", () => {
    open();
    // A dialog that opens without moving focus leaves a keyboard user tabbing
    // through the page underneath to reach it.
    expect(document.activeElement).toBe(screen.getByLabelText("email"));
  });

  test("the opening focus skips a control that is not on the page", () => {
    // A `hidden` file input — the picker an Import button clicks — matches the
    // focusable selector and cannot take focus, and `focus()` on it is a
    // silent no-op. The address book opens with exactly that as its first
    // element (D2.06), so before this the caret stayed on the page behind.
    render(
      <Modal title="Contacts" onClose={() => {}}>
        <input type="file" hidden aria-label="picker" />
        <button>Import</button>
      </Modal>,
    );
    expect(document.activeElement).toBe(screen.getByText("Import"));
  });

  test("Tab cannot leave it", () => {
    open();
    const send = screen.getByText("Send");
    send.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    // From the last control, forward wraps to the first rather than escaping
    // to whatever is behind the overlay.
    expect(document.activeElement).toBe(screen.getByLabelText("email"));

    screen.getByLabelText("email").focus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(send);
  });

  test("it names itself for a screen reader", () => {
    open();
    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(dialog.getAttribute("aria-label")).toBe("Invite a colleague");
  });

  test("a click on the backdrop dismisses, a click inside does not", () => {
    const onClose = vi.fn();
    open(onClose);
    fireEvent.mouseDown(screen.getByRole("dialog"));
    expect(onClose).not.toHaveBeenCalled();
    // The overlay is the dialog's parent.
    fireEvent.mouseDown(screen.getByRole("dialog").parentElement!);
    expect(onClose).toHaveBeenCalledOnce();
  });
});
