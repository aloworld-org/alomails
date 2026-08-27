// The keys that exist in English and not yet in Dutch or French.
//
// **The list is empty, and the intent is that it stays that way.** It began at
// 588 — the drift of every module that shipped without a parity test — and was
// worked down to nothing. `locale.test.ts` fails when a key outside this list
// is missing a translation, so with nothing on it the rule is now simply:
// every string exists in all three languages.
//
// Adding a line here is a deliberate exemption, not a convenience. It is the
// right move when a stream needs to land English strings today and cannot
// write the translations in the same change — three streams push to this
// branch, and a red build over strings nobody was asked to write blocks work
// that has nothing to do with translation. It is the wrong move as a habit,
// which is how the first 588 accumulated.
//
// **Do not machine-translate in bulk.** The English carries deliberate
// phrasing — "'Left' is what a person did, said plainly and without a
// euphemism" — and a bulk pass loses exactly the part that was thought about.
// For a suite sold in Belgium and the Netherlands, mediocre Dutch reads as a
// translated foreign product, which is what alo positions against. Draft with
// an agent if that helps, and have somebody who speaks the language approve.
//
// Two conventions the existing catalogs follow, worth knowing before adding to
// them. Both languages address people formally — "u", "vous" — throughout.
// And the product's own type names are not translated: Space, Base, Sheet and
// Doc are called that in every language, while the words around them move.
//
// German (`de.ts`) is newer and deliberately partial: it ships complete
// modules per iteration (M4.1) and is ratcheted per shipped module in
// `locale.test.ts`, not through this list. Same conventions: "Sie"
// throughout, type names untranslated.
export const UNTRANSLATED: readonly string[] = [
] as const;
