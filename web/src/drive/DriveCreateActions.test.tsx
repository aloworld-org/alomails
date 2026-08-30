import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { DriveCreateActions } from "./DriveCreateActions";

afterEach(cleanup);

const labels = {
  createDocument: "New document",
  more: "More creation options",
  sheet: "Sheet",
  word: "Word document",
  slides: "Presentation",
  folder: "Folder",
  upload: "Upload files",
};

function setup() {
  const actions = {
    onCreateDocument: vi.fn(),
    onCreateSheet: vi.fn(),
    onCreateWord: vi.fn(),
    onCreateSlides: vi.fn(),
    onCreateFolder: vi.fn(),
    onUpload: vi.fn(),
  };
  render(<DriveCreateActions labels={labels} {...actions} />);
  return actions;
}

describe("DriveCreateActions", () => {
  test("makes a document the immediate primary action", () => {
    const actions = setup();

    fireEvent.click(
      screen.getByRole("button", { name: labels.createDocument }),
    );

    expect(actions.onCreateDocument).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  test("keeps every alternative creation path in one labelled chooser", () => {
    setup();

    fireEvent.click(screen.getByRole("button", { name: labels.more }));

    const menu = screen.getByRole("menu");
    expect(menu).toBeTruthy();
    expect(menu.parentElement).toBe(document.body);
    for (const label of [
      labels.sheet,
      labels.word,
      labels.slides,
      labels.folder,
      labels.upload,
    ]) {
      expect(screen.getByRole("menuitem", { name: label })).toBeTruthy();
    }
  });

  test.each([
    [labels.sheet, "onCreateSheet"],
    [labels.word, "onCreateWord"],
    [labels.slides, "onCreateSlides"],
    [labels.folder, "onCreateFolder"],
    [labels.upload, "onUpload"],
  ] as const)("routes %s to only its own action", (label, actionName) => {
    const actions = setup();

    fireEvent.click(screen.getByRole("button", { name: labels.more }));
    fireEvent.click(screen.getByRole("menuitem", { name: label }));

    for (const [name, action] of Object.entries(actions)) {
      if (name === actionName) {
        expect(action).toHaveBeenCalledOnce();
      } else {
        expect(action).not.toHaveBeenCalled();
      }
    }
    expect(screen.queryByRole("menu")).toBeNull();
  });

  test("keeps upload visible but unavailable while another upload is running", () => {
    render(
      <DriveCreateActions
        labels={labels}
        onCreateDocument={vi.fn()}
        onCreateSheet={vi.fn()}
        onCreateWord={vi.fn()}
        onCreateSlides={vi.fn()}
        onCreateFolder={vi.fn()}
        onUpload={vi.fn()}
        uploadDisabled
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: labels.more }));

    expect(
      (
        screen.getByRole("menuitem", {
          name: labels.upload,
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });
});
