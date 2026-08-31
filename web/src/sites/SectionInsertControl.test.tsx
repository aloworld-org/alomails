import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { strings } from "../i18n";
import { SectionInsertControl } from "./SectionInsertControl";

test("opens section insertion from inside the section stack", () => {
  const onAdd = vi.fn();
  render(
    <SectionInsertControl disabled={false} expanded={false} onAdd={onAdd} />,
  );
  fireEvent.click(
    screen.getByRole("button", { name: strings.sitesAddSection }),
  );
  expect(onAdd).toHaveBeenCalledOnce();
});
