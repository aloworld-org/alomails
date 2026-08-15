// The assistant's admin screen (ADR 0040, S3.02d). What these tests pin is
// the item's letter: the switch and the budget live on ONE screen, and the
// sentence "anyone on the internet will be able to read this" stands above
// the publish button on the panel AND again in the dialog — every time,
// with no way to dismiss it.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { AssistantView } from "./AssistantView";

const mocks = vi.hoisted(() => ({
  site: vi.fn(),
  chatSettings: vi.fn(),
  setChatSettings: vi.fn(),
  chatKnowledge: vi.fn(),
  addChatKnowledge: vi.fn(),
  removeChatKnowledge: vi.fn(),
  chatActions: vi.fn(),
  chatAppearance: vi.fn(),
  setChatAppearance: vi.fn(),
  chatAppearancePreview: vi.fn(),
  themePresets: vi.fn(),
  pages: vi.fn(),
  page: vi.fn(),
  driveList: vi.fn(),
  driveUploadBlob: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, useSitesApi: () => mocks };
});

vi.mock("../jmap", () => ({
  useJmapClient: () => ({
    driveList: mocks.driveList,
    driveUploadBlob: mocks.driveUploadBlob,
  }),
}));

const settings = {
  enabled: false,
  monthlyCeilingCents: 1000,
  defaultCeilingCents: 1000,
  month: "2026-08",
  spentCents: 0,
  ceilingHit: false,
};

const appearance = {
  botName: null,
  avatarBlobId: null,
  welcome: null,
  suggestedQuestions: [],
  tone: "neutral",
  toneNote: null,
  launcherCorner: "right",
  launcherIcon: "chat",
  autoOpen: false,
  offlineMessage: null,
  accent: "primary",
  limits: {
    botNameChars: 60,
    welcomeChars: 400,
    suggestedQuestions: 3,
    suggestedQuestionChars: 160,
    toneNoteChars: 500,
    offlineMessageChars: 300,
  },
  defaults: {
    botName: "Ask us anything",
    welcome: "Hello! Ask me anything about what is published on this site.",
    offlineMessage: "The assistant is not available right now.",
  },
};

function mount() {
  return render(
    <MemoryRouter initialEntries={["/sites/site-1/assistant"]}>
      <Routes>
        <Route path="/sites/:siteId/assistant" element={<AssistantView />} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.site.mockResolvedValue({ id: "site-1", name: "Axon", theme: {} });
  mocks.chatSettings.mockResolvedValue(settings);
  mocks.chatKnowledge.mockResolvedValue([]);
  mocks.chatActions.mockResolvedValue([]);
  mocks.chatAppearance.mockResolvedValue(appearance);
  mocks.chatAppearancePreview.mockResolvedValue("<!doctype html><p>preview</p>");
  mocks.themePresets.mockResolvedValue([]);
  mocks.pages.mockResolvedValue([]);
  mocks.driveList.mockResolvedValue([]);
});

afterEach(cleanup);

describe("the assistant screen", () => {
  test("the switch and the budget are one screen, saved in cents", async () => {
    mocks.setChatSettings.mockResolvedValue({
      ...settings,
      enabled: true,
      monthlyCeilingCents: 2500,
    });
    mount();

    // The switch and the budget field render together, the budget pre-filled
    // with the default rather than blank.
    const toggle = await screen.findByLabelText(strings.sitesAssistantEnable);
    const budget = screen.getByLabelText(strings.sitesAssistantBudgetLabel);
    expect((budget as HTMLInputElement).value).toBe("10");

    fireEvent.click(toggle);
    fireEvent.change(budget, { target: { value: "25" } });
    fireEvent.click(screen.getByRole("button", { name: strings.sitesAssistantSave }));

    await screen.findByText(strings.sitesAssistantSaved);
    expect(mocks.setChatSettings).toHaveBeenCalledWith("site-1", true, 2500);
  });

  test("a used-up budget is said out loud", async () => {
    mocks.chatSettings.mockResolvedValue({
      ...settings,
      enabled: true,
      spentCents: 1000,
      ceilingHit: true,
    });
    mount();
    expect(await screen.findByText(strings.sitesAssistantCeilingHit)).toBeTruthy();
  });

  test("the internet sentence stands above the publish button, on the panel and in the dialog", async () => {
    mocks.chatKnowledge.mockResolvedValue([
      {
        id: "src-1",
        docNodeId: "doc-1",
        title: "Price list",
        trashed: false,
        addedBy: "user-1",
        addedAt: "2026-08-15T08:00:00Z",
      },
    ]);
    mocks.driveList.mockResolvedValue([
      {
        id: "doc-2",
        kind: "doc",
        name: "Opening hours",
        blobId: "blob-2",
        trashed: false,
      },
    ]);
    mocks.addChatKnowledge.mockResolvedValue({
      id: "src-2",
      docNodeId: "doc-2",
      title: "Opening hours",
      trashed: false,
      addedBy: "user-1",
      addedAt: "2026-08-15T09:00:00Z",
    });
    mount();

    // On the panel: the sentence, then the button.
    expect(await screen.findByText(strings.sitesAssistantInternetWarning)).toBeTruthy();
    expect(screen.getByText("Price list")).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAssistantPublishDocument }),
    );

    // In the dialog: the sentence again, above the confirm — and the confirm
    // stays disabled until a document is actually chosen.
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(strings.sitesAssistantInternetWarning)).toBeTruthy();
    const confirm = within(dialog).getByRole("button", {
      name: strings.sitesAssistantPickerConfirm,
    });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(await within(dialog).findByText("Opening hours"));
    expect((confirm as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(confirm);

    await waitFor(() =>
      expect(mocks.addChatKnowledge).toHaveBeenCalledWith("site-1", "doc-2"),
    );
    // The new source appears without a reload; the dialog is gone.
    expect(await screen.findByText("Opening hours")).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("the transcript shows each action, the fact it used, and its page", async () => {
    mocks.chatActions.mockResolvedValue([
      {
        id: "act-3",
        kind: "lead_saved",
        fact: null,
        slotAt: null,
        citations: [],
        occurredAt: "2026-08-15T10:30:00Z",
      },
      {
        id: "act-2",
        kind: "booked",
        fact: "Intro call",
        slotAt: "2026-08-20T09:00:00Z",
        citations: [],
        occurredAt: "2026-08-15T10:10:00Z",
      },
      {
        id: "act-1",
        kind: "answered",
        fact: null,
        slotAt: null,
        citations: [
          { title: "Pricing", path: "/pricing" },
          { title: "Opening hours", path: null },
        ],
        occurredAt: "2026-08-15T10:00:00Z",
      },
      {
        // A kind a later release added: skipped, never broken on.
        id: "act-0",
        kind: "paid",
        fact: null,
        slotAt: null,
        citations: [],
        occurredAt: "2026-08-15T09:00:00Z",
      },
    ]);
    mount();

    // The answer names the pages the fact came from, path and all.
    expect(
      await screen.findByText(
        strings.sitesAssistantDidAnsweredUsing("Pricing (/pricing), Opening hours"),
      ),
    ).toBeTruthy();
    // The booking names the published service it used.
    expect(
      screen.getByText((text) => text.startsWith("Booked “Intro call” for")),
    ).toBeTruthy();
    expect(screen.getByText(strings.sitesAssistantDidLeadSaved)).toBeTruthy();
    expect(screen.queryByText(strings.sitesAssistantDidEmpty)).toBeNull();
  });

  test("an assistant that did nothing yet says so", async () => {
    mount();
    expect(await screen.findByText(strings.sitesAssistantDidEmpty)).toBeTruthy();
  });

  test("withdrawing a source only ever touches that source", async () => {
    mocks.chatKnowledge.mockResolvedValue([
      {
        id: "src-1",
        docNodeId: "doc-1",
        title: "Price list",
        trashed: true,
        addedBy: "user-1",
        addedAt: "2026-08-15T08:00:00Z",
      },
    ]);
    mocks.removeChatKnowledge.mockResolvedValue(undefined);
    mount();

    // A trashed document is flagged, not hidden — the binding stays visible
    // and removable.
    expect(await screen.findByText(strings.sitesAssistantTrashed)).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", {
        name: strings.sitesAssistantWithdraw("Price list"),
      }),
    );
    await waitFor(() =>
      expect(mocks.removeChatKnowledge).toHaveBeenCalledWith("site-1", "src-1"),
    );
    expect(screen.queryByText("Price list")).toBeNull();
  });
});
