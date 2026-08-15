// The assistant's appearance panel (ADR 0040 §5, S3.02g). What these tests
// pin is the queue item's letter: the welcome box opens PRE-FILLED with the
// written default rather than empty; suggested questions are drafted from
// the site's own pages into the empty slots; the accessibility facts —
// measured contrast among them — are shown in the screen; and the live
// preview renders the SAME draft the save would store, through the server.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { AssistantAppearance } from "./AssistantAppearance";
import { SitesError } from "./api";
import type { SiteDetail } from "./types";

const mocks = vi.hoisted(() => ({
  chatAppearance: vi.fn(),
  setChatAppearance: vi.fn(),
  chatAppearancePreview: vi.fn(),
  themePresets: vi.fn(),
  pages: vi.fn(),
  page: vi.fn(),
  driveUploadBlob: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, useSitesApi: () => mocks };
});

vi.mock("../jmap", () => ({
  useJmapClient: () => ({ driveUploadBlob: mocks.driveUploadBlob }),
}));

const defaults = {
  botName: "Ask us anything",
  welcome: "Hello! Ask me anything about what is published on this site.",
  offlineMessage: "The assistant is not available right now.",
};

const view = {
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
  defaults,
};

// #1a1a1a on #ffffff: ratio ≈ 17.9 — the number the screen must show.
const preset = {
  id: "clean",
  name: "Clean",
  palette: {
    background: "#ffffff",
    surface: "#f5f5f5",
    text: "#1a1a1a",
    mutedText: "#555555",
    primary: "#1a1a1a",
    onPrimary: "#ffffff",
    border: "#dddddd",
  },
  typography: { headingFamily: "serif", bodyFamily: "sans-serif", headingWeight: 700 },
};

const site = {
  id: "site-1",
  name: "Axon",
  theme: { preset: "clean" },
} as unknown as SiteDetail;

function mount() {
  return render(<AssistantAppearance siteId="site-1" site={site} />);
}

beforeEach(() => {
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.chatAppearance.mockResolvedValue(view);
  mocks.themePresets.mockResolvedValue([preset]);
  mocks.chatAppearancePreview.mockResolvedValue("<!doctype html><h2>Marie</h2>");
  mocks.pages.mockResolvedValue([]);
});

afterEach(cleanup);

describe("the appearance panel", () => {
  test("the welcome box opens pre-filled with the written default, and saving it untouched stores no override", async () => {
    mocks.setChatAppearance.mockResolvedValue(view);
    mount();

    const welcome = await screen.findByLabelText(strings.sitesAssistantWelcomeLabel);
    expect((welcome as HTMLTextAreaElement).value).toBe(defaults.welcome);
    expect(screen.getByText(strings.sitesAssistantWelcomeDefaultNote)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: strings.sitesAssistantAppearanceSave }));
    await screen.findByText(strings.sitesAssistantSaved);
    expect(mocks.setChatAppearance).toHaveBeenCalledWith(
      "site-1",
      expect.objectContaining({ welcome: null }),
    );
  });

  test("an edited welcome is stored verbatim and the default note goes away", async () => {
    mocks.setChatAppearance.mockResolvedValue({
      ...view,
      welcome: "Hi, I'm Marie.",
    });
    mount();

    const welcome = await screen.findByLabelText(strings.sitesAssistantWelcomeLabel);
    fireEvent.change(welcome, { target: { value: "Hi, I'm Marie." } });
    expect(screen.queryByText(strings.sitesAssistantWelcomeDefaultNote)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: strings.sitesAssistantAppearanceSave }));
    await screen.findByText(strings.sitesAssistantSaved);
    expect(mocks.setChatAppearance).toHaveBeenCalledWith(
      "site-1",
      expect.objectContaining({ welcome: "Hi, I'm Marie." }),
    );
  });

  test("suggested questions are drafted from the site's own pages into the empty slots", async () => {
    mocks.pages.mockResolvedValue([{ id: "page-1" }]);
    mocks.page.mockResolvedValue({
      id: "page-1",
      sections: {
        schema_version: 1,
        sections: [
          {
            type: "faq",
            items: [{ question: "When are you open?", answer: "Nine to five." }],
          },
          { type: "pricing", tiers: [] },
        ],
      },
    });
    mount();

    // The first slot already holds the owner's own question — it is kept.
    const first = await screen.findByLabelText(strings.sitesAssistantQuestionLabel(1));
    fireEvent.change(first, { target: { value: "Do you deliver?" } });

    fireEvent.click(
      screen.getByRole("button", { name: strings.sitesAssistantSuggestFromSite }),
    );
    await screen.findByText(strings.sitesAssistantSuggestedApplied);

    expect((first as HTMLInputElement).value).toBe("Do you deliver?");
    const second = screen.getByLabelText(strings.sitesAssistantQuestionLabel(2));
    const third = screen.getByLabelText(strings.sitesAssistantQuestionLabel(3));
    expect((second as HTMLInputElement).value).toBe("When are you open?");
    expect((third as HTMLInputElement).value).toBe(
      strings.sitesAssistantSuggestedPricing,
    );
  });

  test("a site with nothing to draft from is told so", async () => {
    mocks.pages.mockResolvedValue([{ id: "page-1" }]);
    mocks.page.mockResolvedValue({
      id: "page-1",
      sections: { schema_version: 1, sections: [{ type: "hero", heading: "Axon" }] },
    });
    mount();

    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesAssistantSuggestFromSite }),
    );
    await screen.findByText(strings.sitesAssistantSuggestedNone);
  });

  test("the accessibility check shows the measured contrast of the chosen colour", async () => {
    mount();
    // primary on onPrimary for the "clean" preset: #1a1a1a / #ffffff ≈ 17.4:1.
    await screen.findByText(
      strings.sitesAssistantA11yContrast((17.4).toLocaleString()),
    );
    expect(screen.getByText(strings.sitesAssistantA11yKeyboard)).toBeTruthy();
  });

  test("the live preview renders the same draft the save would store", async () => {
    mount();

    const name = await screen.findByLabelText(strings.sitesAssistantBotNameLabel);
    fireEvent.change(name, { target: { value: "Marie" } });

    await waitFor(() =>
      expect(mocks.chatAppearancePreview).toHaveBeenCalledWith(
        "site-1",
        expect.objectContaining({ botName: "Marie", welcome: null }),
      ),
    );
    const frame = screen.getByTitle(strings.sitesAssistantPreviewFrameTitle);
    await waitFor(() =>
      expect((frame as HTMLIFrameElement).getAttribute("srcdoc")).toContain("Marie"),
    );
  });

  test("a refused save shows the server's own sentence", async () => {
    mocks.setChatAppearance.mockRejectedValue(
      new SitesError(422, "bot_name must be at most 60 characters"),
    );
    mount();

    fireEvent.click(
      await screen.findByRole("button", { name: strings.sitesAssistantAppearanceSave }),
    );
    await screen.findByText("bot_name must be at most 60 characters");
  });
});
