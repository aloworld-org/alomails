// Resolving a message's category keywords against the account's catalog. Pure;
// the catalog (a handful of entries) is the source of a category's name/color,
// and a message carries membership as its `$category_<id>` keywords.
import type { Category, EmailHeaders } from "../jmap";

/** The categories a message is tagged with, in catalog order. */
export function categoriesOf(
  keywords: Record<string, boolean>,
  catalog: Category[],
): Category[] {
  return catalog.filter((c) => keywords[c.keyword] === true);
}

/** Ids of the categories present on ANY message of a conversation (union). */
export function threadCategoryIds(messages: { keywords: Record<string, boolean> }[], catalog: Category[]): Set<string> {
  const ids = new Set<string>();
  for (const m of messages) {
    for (const c of catalog) {
      if (m.keywords[c.keyword] === true) ids.add(c.id);
    }
  }
  return ids;
}

/** The categories on a header row (list rendering). */
export function rowCategories(row: { latest: EmailHeaders }, catalog: Category[]): Category[] {
  return categoriesOf(row.latest.keywords, catalog);
}
