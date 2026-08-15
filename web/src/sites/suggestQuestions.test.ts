// The question drafter (S3.02g): FAQ entries verbatim and in order, one
// canonical question per present section kind, deduplicated, bounded, and
// honest about having nothing.
import { describe, expect, test } from "vitest";

import { strings } from "../i18n";
import { accentContrast, contrastRatio } from "./accentContrast";
import { draftSuggestedQuestions } from "./suggestQuestions";
import type { Section } from "./sections";

function page(...sections: Section[]) {
  return { sections: { sections } };
}

const faq = (...questions: string[]): Section =>
  ({
    type: "faq",
    items: questions.map((question) => ({ question, answer: "…" })),
  }) as Section;

describe("draftSuggestedQuestions", () => {
  test("faq questions come first, verbatim and in page order", () => {
    const drafts = draftSuggestedQuestions(
      [
        page(faq("When are you open?"), { type: "pricing", tiers: [] } as unknown as Section),
        page(faq("Do you deliver?")),
      ],
      3,
      160,
    );
    expect(drafts).toEqual([
      "When are you open?",
      "Do you deliver?",
      strings.sitesAssistantSuggestedPricing,
    ]);
  });

  test("section kinds add one canonical question each, capped at the limit", () => {
    const drafts = draftSuggestedQuestions(
      [
        page(
          { type: "pricing", tiers: [] } as unknown as Section,
          { type: "booking", booking_id: "b1" } as unknown as Section,
          { type: "catalog", catalog_id: "c1" } as unknown as Section,
          { type: "contact_form" } as unknown as Section,
        ),
      ],
      3,
      160,
    );
    expect(drafts).toEqual([
      strings.sitesAssistantSuggestedPricing,
      strings.sitesAssistantSuggestedBooking,
      strings.sitesAssistantSuggestedCatalog,
    ]);
  });

  test("duplicates, blanks, and over-cap questions are dropped", () => {
    const drafts = draftSuggestedQuestions(
      [page(faq("When are you open?", "  when are you open? ", "   ", "x".repeat(200)))],
      3,
      160,
    );
    expect(drafts).toEqual(["When are you open?"]);
  });

  test("a site with nothing to draft from answers nothing rather than filler", () => {
    expect(
      draftSuggestedQuestions(
        [page({ type: "hero", heading: "Axon" } as unknown as Section)],
        3,
        160,
      ),
    ).toEqual([]);
  });
});

describe("accentContrast", () => {
  const palette = {
    background: "#ffffff",
    surface: "#f5f5f5",
    text: "#1a1a1a",
    mutedText: "#555555",
    primary: "#1a1a1a",
    onPrimary: "#ffffff",
    border: "#dddddd",
  };

  test("measures the same role pairs the server proves", () => {
    // Black-ish on white is ≈ 17.4:1; every accent on this palette passes AA.
    expect(accentContrast("primary", palette)).toBeCloseTo(17.4, 1);
    expect(accentContrast("text", palette)).toBeCloseTo(17.4, 1);
    const surface = accentContrast("surface", palette);
    expect(surface).not.toBeNull();
    expect(surface as number).toBeGreaterThanOrEqual(4.5);
  });

  test("a malformed colour yields null, never a made-up number", () => {
    expect(contrastRatio("#zzzzzz", "#ffffff")).toBeNull();
    expect(contrastRatio("red", "#ffffff")).toBeNull();
  });
});
