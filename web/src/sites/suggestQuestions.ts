// Drafting the assistant's suggested opening questions from the site's own
// pages (S3.02g): FAQ entries are offered verbatim — they are literally the
// questions this business gets asked — and the presence of pricing, booking,
// catalog, or contact sections adds one canonical question each. Every draft
// is editable text in the form; nothing here is stored until the owner saves.
//
// Deterministic and local: no model call, no network — the same "manual
// sibling beside AI" posture the queue demands everywhere else.
import { strings } from "../i18n";
import type { Section } from "./sections";

/** A page as the drafter needs it — its ordered typed sections. */
export interface DraftablePage {
  sections: { sections: Section[] };
}

/** Case- and whitespace-insensitive identity for deduplication. */
function key(question: string): string {
  return question.trim().toLowerCase();
}

/**
 * Up to `limit` suggested questions drafted from the pages' own content,
 * each within `maxChars` (the server's per-question cap). FAQ questions
 * come first, in page order; section-kind questions follow. May answer
 * fewer than `limit`, or none — the screen says so rather than padding
 * with filler.
 */
export function draftSuggestedQuestions(
  pages: DraftablePage[],
  limit: number,
  maxChars: number,
): string[] {
  const drafts: string[] = [];
  const seen = new Set<string>();
  const kinds = new Set<string>();

  const offer = (question: string) => {
    const trimmed = question.trim();
    if (trimmed === "" || trimmed.length > maxChars) return;
    if (seen.has(key(trimmed))) return;
    seen.add(key(trimmed));
    drafts.push(trimmed);
  };

  for (const page of pages) {
    for (const section of page.sections.sections) {
      kinds.add(section.type);
      if (section.type === "faq") {
        for (const item of section.items) offer(item.question);
      }
    }
  }

  if (kinds.has("pricing")) offer(strings.sitesAssistantSuggestedPricing);
  if (kinds.has("booking")) offer(strings.sitesAssistantSuggestedBooking);
  if (kinds.has("tickets")) offer(strings.sitesAssistantSuggestedTickets);
  if (kinds.has("shop")) offer(strings.sitesAssistantSuggestedShop);
  if (kinds.has("catalog")) offer(strings.sitesAssistantSuggestedCatalog);
  if (kinds.has("contact_form")) offer(strings.sitesAssistantSuggestedContact);

  return drafts.slice(0, limit);
}
