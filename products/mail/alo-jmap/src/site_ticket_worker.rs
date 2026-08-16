//! The sweep that makes paid ticket sales good (ADR 0041, item S3.04d): for
//! every paid, unfulfilled order, mint the buyer's ticket, raise and settle
//! the invoice in Billing and hand the buyer to CRM — each through the owning
//! module's own door, all inside [`alo_store::site_ticket_fulfil`].
//!
//! This module is only the pacing and the words: the store claims
//! at-most-once (the fulfilment row is the claim), and this sweep resolves
//! the site's language into the invoice unit, the payment-method line and
//! the CRM seed — the same one-per-surface word tables the chat widget's
//! lead capture keeps, because the store composes documents from words it is
//! handed and invents none.
//!
//! Nothing that reaches a log here carries a buyer's name or address: only
//! ids and coarse errors (Law 1). No mail leaves from this sweep — the
//! ticket email is its sibling's act ([`crate::site_ticket_mail`], ADR
//! 0050), which claims a sale only after this one has recorded it.

use alo_store::{PipelineSeed, StageSeed, Store, TicketFulfilWords};

/// How many paid orders one sweep round claims. Small: each one is a handful
/// of Billing writes, and a backlog of ticket sales is a good problem the
/// next round absorbs thirty seconds later.
const BATCH: i64 = 10;

/// The words of one language: what fulfilment prints, and where a first lead
/// lands.
struct Words {
    fulfil: TicketFulfilWords,
    pipeline: &'static str,
    stages: [&'static str; 5],
}

static EN_WORDS: Words = Words {
    fulfil: TicketFulfilWords {
        unit: "ticket",
        fallback_item: "Event ticket",
        payment_method: "Hosted checkout",
        crm_title: "Ticket sale",
    },
    pipeline: "Sales",
    stages: ["New", "Qualified", "Proposal", "Won", "Lost"],
};

static FR_WORDS: Words = Words {
    fulfil: TicketFulfilWords {
        unit: "billet",
        fallback_item: "Billet d'événement",
        payment_method: "Paiement hébergé",
        crm_title: "Vente de billets",
    },
    pipeline: "Ventes",
    stages: ["Nouveau", "Qualifié", "Proposition", "Gagné", "Perdu"],
};

static NL_WORDS: Words = Words {
    fulfil: TicketFulfilWords {
        unit: "ticket",
        fallback_item: "Evenementticket",
        payment_method: "Externe betaalpagina",
        crm_title: "Ticketverkoop",
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

/// Fulfils every paid, unfulfilled ticket order. Returns how many sales were
/// made good this round.
pub async fn run_due(store: &Store) -> usize {
    let mut fulfilled = 0;
    loop {
        let claims = match store.claim_ticket_fulfilments(BATCH).await {
            Ok(claims) => claims,
            Err(error) => {
                tracing::warn!(%error, "ticket fulfilment sweep: claim failed");
                return fulfilled;
            }
        };
        let batch_len = claims.len();
        for claim in &claims {
            let words = words_for(&claim.default_locale);
            let seed = seed_for(words);
            match store
                .fulfil_claimed_ticket(claim, &words.fulfil, &seed)
                .await
            {
                Ok(outcome) => {
                    fulfilled += 1;
                    if !outcome.invoiced {
                        // The reason is on the row (invoice_note); the log
                        // only says it happened.
                        tracing::warn!(
                            order = claim.order.as_str(),
                            "ticket fulfilment: sale fulfilled without an invoice"
                        );
                    }
                }
                Err(error) => {
                    // The claim stands (at-most-once); the row's empty
                    // columns are the visible trace.
                    tracing::warn!(
                        %error,
                        order = claim.order.as_str(),
                        "ticket fulfilment: could not record the fulfilment"
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
        assert_eq!(words_for("fr-BE").pipeline, "Ventes");
        assert_eq!(words_for("NL").pipeline, "Verkoop");
        assert_eq!(words_for("de").pipeline, "Sales");
        assert_eq!(words_for("").pipeline, "Sales");
    }

    #[test]
    fn the_seed_ends_with_the_winning_and_the_losing_column() {
        let seed = seed_for(&EN_WORDS);
        assert_eq!(seed.stages.len(), 5);
        assert!(seed.stages[3].is_won && !seed.stages[3].is_lost);
        assert!(seed.stages[4].is_lost && !seed.stages[4].is_won);
    }
}
