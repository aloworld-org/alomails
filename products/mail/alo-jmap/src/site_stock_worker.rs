//! The sweep that puts paid stock sales on paper (ADR 0041, item S3.05a2):
//! for every paid, uninvoiced stock order, raise and settle the invoice in
//! Billing — delivery as its own line, VAT carved out of the exact charge —
//! and hand the buyer to CRM, each through the owning module's own door, all
//! inside [`alo_store::site_stock_fulfil`].
//!
//! The goods themselves moved when the payment settled (the claim through
//! Inventory's seam recorded the outbound movement); this sweep owes the
//! sale nothing but its paper. Like its ticket sibling
//! ([`crate::site_ticket_worker`]), this module is only the pacing and the
//! words: the store claims at-most-once (the fulfilment row is the claim),
//! and the site's language resolves the invoice words and the CRM seed —
//! the store composes documents from words it is handed and invents none.
//!
//! Nothing that reaches a log here carries a buyer's name or address: only
//! ids and coarse errors (Law 1).

use alo_store::{PipelineSeed, StageSeed, StockFulfilWords, Store};

/// How many paid orders one sweep round claims. Small: each one is a handful
/// of Billing writes, and a backlog of sales is a good problem the next
/// round absorbs thirty seconds later.
const BATCH: i64 = 10;

/// The words of one language: what fulfilment prints, and where a first lead
/// lands.
struct Words {
    fulfil: StockFulfilWords,
    pipeline: &'static str,
    stages: [&'static str; 5],
}

static EN_WORDS: Words = Words {
    fulfil: StockFulfilWords {
        unit: "piece",
        fallback_item: "Shop item",
        shipping: "Shipping",
        payment_method: "Hosted checkout",
        crm_title: "Shop sale",
    },
    pipeline: "Sales",
    stages: ["New", "Qualified", "Proposal", "Won", "Lost"],
};

static FR_WORDS: Words = Words {
    fulfil: StockFulfilWords {
        unit: "pièce",
        fallback_item: "Article de la boutique",
        shipping: "Livraison",
        payment_method: "Paiement hébergé",
        crm_title: "Vente en ligne",
    },
    pipeline: "Ventes",
    stages: ["Nouveau", "Qualifié", "Proposition", "Gagné", "Perdu"],
};

static NL_WORDS: Words = Words {
    fulfil: StockFulfilWords {
        unit: "stuk",
        fallback_item: "Webshopartikel",
        shipping: "Verzending",
        payment_method: "Externe betaalpagina",
        crm_title: "Webshopverkoop",
    },
    pipeline: "Verkoop",
    stages: [
        "Nieuw",
        "Gekwalificeerd",
        "Voorstel",
        "Gewonnen",
        "Verloren",
    ],
};

/// The words for a site's default locale, falling back to English — the same
/// primary-subtag rule every sites surface resolves by.
fn words_for(tag: &str) -> &'static Words {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR_WORDS,
        "nl" => &NL_WORDS,
        _ => &EN_WORDS,
    }
}

/// The first-use board in the site's language, with the flags CRM's design
/// fixes: three open columns, then the winning and the losing one.
fn seed_for(words: &Words) -> PipelineSeed {
    let flags = [
        (false, false),
        (false, false),
        (false, false),
        (true, false),
        (false, true),
    ];
    PipelineSeed {
        name: words.pipeline.to_owned(),
        stages: words
            .stages
            .iter()
            .zip(flags)
            .map(|(name, (is_won, is_lost))| StageSeed {
                name: (*name).to_owned(),
                is_won,
                is_lost,
            })
            .collect(),
    }
}

/// Fulfils every paid, uninvoiced stock order. Returns how many sales were
/// put on paper this round.
pub async fn run_due(store: &Store) -> usize {
    let mut fulfilled = 0;
    loop {
        let claims = match store.claim_stock_fulfilments(BATCH).await {
            Ok(claims) => claims,
            Err(error) => {
                tracing::warn!(%error, "stock fulfilment sweep: claim failed");
                return fulfilled;
            }
        };
        let batch_len = claims.len();
        for claim in &claims {
            let words = words_for(&claim.default_locale);
            let seed = seed_for(words);
            match store
                .fulfil_claimed_stock(claim, &words.fulfil, &seed)
                .await
            {
                Ok(outcome) => {
                    fulfilled += 1;
                    if !outcome.invoiced {
                        // The reason is on the row (invoice_note); the log
                        // only says it happened.
                        tracing::warn!(
                            order = claim.order.as_str(),
                            "stock fulfilment: sale fulfilled without an invoice"
                        );
                    }
                }
                Err(error) => {
                    // The claim stands (at-most-once); the row's empty
                    // columns are the visible trace.
                    tracing::warn!(
                        %error,
                        order = claim.order.as_str(),
                        "stock fulfilment: could not record the fulfilment"
                    );
                }
            }
        }
        if batch_len < BATCH as usize {
            return fulfilled;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_locale_resolves_by_primary_subtag_with_an_english_fallback() {
        assert_eq!(words_for("fr-BE").fulfil.shipping, "Livraison");
        assert_eq!(words_for("NL").fulfil.shipping, "Verzending");
        assert_eq!(words_for("de").fulfil.shipping, "Shipping");
        assert_eq!(words_for("").fulfil.shipping, "Shipping");
    }

    #[test]
    fn the_seed_ends_with_the_winning_and_the_losing_column() {
        let seed = seed_for(&EN_WORDS);
        assert_eq!(seed.stages.len(), 5);
        assert!(seed.stages[3].is_won && !seed.stages[3].is_lost);
        assert!(seed.stages[4].is_lost && !seed.stages[4].is_won);
    }
}
