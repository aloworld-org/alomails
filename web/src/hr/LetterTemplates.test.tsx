import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { LetterTemplatesView } from "./LetterTemplatesView";

const calls: Array<{ method: string; body: unknown }> = [];
const stored = {
  id: "letter-1",
  name: "Employment confirmation",
  subject: "Confirmation for {{employee.name}}",
  body: "This confirms that {{employee.name}} works here.",
  fields: ["employee.name"],
  createdBy: "user-1",
  createdAt: "2026-08-11T09:00:00Z",
  updatedAt: "2026-08-11T09:00:00Z",
};

const authorizedFetch = vi.fn((_: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  const body =
    typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
  calls.push({ method, body });
  return Promise.resolve(
    new Response(
      JSON.stringify(
        method === "GET"
          ? { templates: [stored], fields: ["employee.name", "company.name"] }
          : { template: { ...stored, id: "letter-2", ...body } },
      ),
      { status: 200, headers: { "content-type": "application/json" } },
    ),
  );
});

vi.mock("../auth", () => ({ useAuth: () => ({ authorizedFetch }) }));

describe("letter templates", () => {
  beforeEach(() => {
    calls.length = 0;
    authorizedFetch.mockClear();
  });

  test("shows approved templates and creates one with a recognised placeholder", async () => {
    render(
      <DialogProvider>
        <LetterTemplatesView />
      </DialogProvider>,
    );
    expect(await screen.findByText("Employment confirmation")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "New template" }));
    fireEvent.change(screen.getByLabelText("Template name"), {
      target: { value: "Reference" },
    });
    fireEvent.change(screen.getByLabelText("Email subject"), {
      target: { value: "Your reference" },
    });
    const wording = screen.getByRole("dialog").querySelector("textarea");
    if (wording === null) throw new Error("letter wording textarea is missing");
    fireEvent.change(wording, {
      target: { value: "Dear" },
    });
    fireEvent.click(screen.getByRole("button", { name: "employee.name" }));
    fireEvent.click(screen.getByRole("button", { name: "Save template" }));
    await waitFor(() =>
      expect(
        calls.some(
          (call) =>
            call.method === "POST" &&
            (call.body as { body?: string }).body === "Dear {{employee.name}}",
        ),
      ).toBe(true),
    );
  });
});
