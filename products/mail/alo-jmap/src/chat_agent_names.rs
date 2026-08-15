//! The words a tenant's default agents are written with (ADR 0034; queue item
//! A1.5; `docs/design/chat-agents.md` § The default set).
//!
//! [`alo_store::chat_agent_seed`] states each default agent's **product and
//! handle** and deliberately no name: an agent called `Websites` in a French
//! tenant is a hardcoded English string in a European product, and the store is
//! where a hardcoded string would be hardest to see. So the names live at the
//! edge, one table per language, and the set a tenant is seeded with is written
//! in the language of whoever opened the agent list first —
//! [`crate::inventory_location_names`]' mechanism, reused whole, down to the
//! `?lang=` the client sends.
//!
//! **A name here is the rail's own word for the module.** Somebody who clicks
//! *Sales* every morning should not have to learn that its agent is called
//! something else; the handle (`@crm`) is the one place the internal word
//! shows, and it shows because people type it. Recognition over recall, applied
//! to a roster.
//!
//! From the moment they are written the agents are **ordinary tenant data**: a
//! tenant who disagrees with a word renames the agent, and nothing here ever
//! overwrites it. That is also why a language added later cannot retranslate an
//! existing tenant's agents — the seed runs once (`chat_agent_seeds`), and a
//! name a human may have edited is not ours to revise.

use alo_store::{AgentProduct, AgentSeed, AgentWords};

/// One agent's words: the product it belongs to, its name, and the one line
/// saying what asking it is good for.
type AgentLine = (AgentProduct, &'static str, &'static str);

/// The words one language writes a tenant's default agents with.
pub struct AgentWordsTable {
    /// BCP 47 tag this table is written in.
    pub lang: &'static str,
    /// One line per product. Order is free — [`agent_seed_for`] looks each
    /// product up rather than reading positionally — but every product must
    /// appear, because the store refuses a seed that is short one.
    pub agents: [AgentLine; 16],
}

/// The default table.
static EN: AgentWordsTable = AgentWordsTable {
    lang: "en",
    agents: [
        (
            AgentProduct::Mail,
            "Mail",
            "Ask about your correspondence: who wrote, what was agreed, what is still owed a reply.",
        ),
        (
            AgentProduct::Agenda,
            "Agenda",
            "Ask about your diary: what is next, when everyone is free, what a meeting is for.",
        ),
        (
            AgentProduct::Tasks,
            "Tasks",
            "Ask what is on your plate, what is overdue, and what a conversation committed you to.",
        ),
        (
            AgentProduct::Chat,
            "Chat",
            "Ask what a conversation decided, who said it, and what came of it.",
        ),
        (
            AgentProduct::Drive,
            "Drive",
            "Ask what a document says, where a file went, and what an attachment contains.",
        ),
        (
            AgentProduct::Sheets,
            "Sheets",
            "Ask about a spreadsheet: what the figures say, what a formula does, and which column needs tidying.",
        ),
        (
            AgentProduct::Billing,
            "Billing",
            "Ask about quotes and invoices: what is unpaid, what a customer owes, what was sent.",
        ),
        (
            AgentProduct::Crm,
            "Sales",
            "Ask about contacts and deals: who we are talking to, what stage it is at, what is next.",
        ),
        (
            AgentProduct::Projects,
            "Projects",
            "Ask about projects: what is running late, who is on what, where the hours went.",
        ),
        (
            AgentProduct::Finance,
            "Finance",
            "Ask about the books: what was booked, what VAT is due, what an expense belongs to.",
        ),
        (
            AgentProduct::Inventory,
            "Inventory",
            "Ask about stock: what is on hand, what is on order, and what needs reordering.",
        ),
        (
            AgentProduct::Hr,
            "People",
            "Ask about staff records: who is away, how much leave is left, what a letter should say.",
        ),
        (
            AgentProduct::Insights,
            "Insights",
            "Ask what the numbers say, why a figure moved, and what a report should show.",
        ),
        (
            AgentProduct::Meet,
            "Meet",
            "Ask what a meeting decided, what was said, and what it left you to do.",
        ),
        (
            AgentProduct::Sites,
            "Websites",
            "Ask about your website: what a page says, what to publish, how it reads in another language.",
        ),
        (
            AgentProduct::Workspace,
            "alo",
            "Ask anything across the whole workspace — it finds the right agent and works across them.",
        ),
    ],
};

/// The French table.
static FR: AgentWordsTable = AgentWordsTable {
    lang: "fr",
    agents: [
        (
            AgentProduct::Mail,
            "Courrier",
            "Posez vos questions sur votre correspondance : qui a écrit, ce qui a été convenu, ce qui attend encore une réponse.",
        ),
        (
            AgentProduct::Agenda,
            "Agenda",
            "Posez vos questions sur votre agenda : ce qui vient, quand chacun est libre, l’objet d’une réunion.",
        ),
        (
            AgentProduct::Tasks,
            "Tâches",
            "Demandez ce qui vous revient, ce qui est en retard, et ce à quoi une conversation vous a engagé.",
        ),
        (
            AgentProduct::Chat,
            "Messagerie",
            "Demandez ce qu’une conversation a décidé, qui l’a dit, et ce qu’il en est advenu.",
        ),
        (
            AgentProduct::Drive,
            "Fichiers",
            "Demandez ce que dit un document, où est passé un fichier, et ce que contient une pièce jointe.",
        ),
        (
            AgentProduct::Sheets,
            "Tableurs",
            "Posez vos questions sur un tableur : ce que disent les chiffres, ce que fait une formule, et quelle colonne demande du rangement.",
        ),
        (
            AgentProduct::Billing,
            "Facturation",
            "Posez vos questions sur les devis et les factures : ce qui reste impayé, ce qu’un client doit, ce qui a été envoyé.",
        ),
        (
            AgentProduct::Crm,
            "Ventes",
            "Posez vos questions sur les contacts et les affaires : à qui nous parlons, à quelle étape, et la suite.",
        ),
        (
            AgentProduct::Projects,
            "Projets",
            "Posez vos questions sur les projets : ce qui prend du retard, qui fait quoi, où sont passées les heures.",
        ),
        (
            AgentProduct::Finance,
            "Finance",
            "Posez vos questions sur les comptes : ce qui a été enregistré, la TVA due, à quoi se rattache une dépense.",
        ),
        (
            AgentProduct::Inventory,
            "Inventaire",
            "Posez vos questions sur le stock : ce qui est disponible, ce qui est commandé, ce qu’il faut réapprovisionner.",
        ),
        (
            AgentProduct::Hr,
            "Personnes",
            "Posez vos questions sur les dossiers du personnel : qui est absent, quels congés restent, ce que doit dire un courrier.",
        ),
        (
            AgentProduct::Insights,
            "Analyses",
            "Demandez ce que disent les chiffres, pourquoi une valeur a bougé, et ce qu’un rapport doit montrer.",
        ),
        (
            AgentProduct::Meet,
            "Réunions",
            "Demandez ce qu’une réunion a décidé, ce qui s’y est dit, et ce qu’elle vous laisse à faire.",
        ),
        (
            AgentProduct::Sites,
            "Sites web",
            "Posez vos questions sur votre site : ce que dit une page, quoi publier, comment cela se lit dans une autre langue.",
        ),
        (
            AgentProduct::Workspace,
            "alo",
            "Posez n’importe quelle question sur l’ensemble de l’espace de travail — il trouve le bon agent et les fait travailler ensemble.",
        ),
    ],
};

/// The Dutch table.
static NL: AgentWordsTable = AgentWordsTable {
    lang: "nl",
    agents: [
        (
            AgentProduct::Mail,
            "E-mail",
            "Vraag naar uw correspondentie: wie schreef, wat is afgesproken, en wat nog op antwoord wacht.",
        ),
        (
            AgentProduct::Agenda,
            "Agenda",
            "Vraag naar uw agenda: wat er aankomt, wanneer iedereen vrij is, en waar een afspraak over gaat.",
        ),
        (
            AgentProduct::Tasks,
            "Taken",
            "Vraag wat er op u wacht, wat te laat is, en waartoe een gesprek u heeft verplicht.",
        ),
        (
            AgentProduct::Chat,
            "Chat",
            "Vraag wat een gesprek heeft besloten, wie het zei, en wat ervan gekomen is.",
        ),
        (
            AgentProduct::Drive,
            "Drive",
            "Vraag wat een document zegt, waar een bestand is gebleven, en wat een bijlage bevat.",
        ),
        (
            AgentProduct::Sheets,
            "Rekenbladen",
            "Vraag naar een rekenblad: wat de cijfers zeggen, wat een formule doet, en welke kolom opgeruimd moet worden.",
        ),
        (
            AgentProduct::Billing,
            "Facturatie",
            "Vraag naar offertes en facturen: wat openstaat, wat een klant verschuldigd is, en wat verstuurd is.",
        ),
        (
            AgentProduct::Crm,
            "Verkoop",
            "Vraag naar contacten en deals: met wie we praten, in welke fase het staat, en wat de volgende stap is.",
        ),
        (
            AgentProduct::Projects,
            "Projecten",
            "Vraag naar projecten: wat uitloopt, wie waaraan werkt, en waar de uren heen gingen.",
        ),
        (
            AgentProduct::Finance,
            "Financiën",
            "Vraag naar de boeken: wat geboekt is, welke btw verschuldigd is, en waar een uitgave bij hoort.",
        ),
        (
            AgentProduct::Inventory,
            "Voorraad",
            "Vraag naar de voorraad: wat er ligt, wat besteld is, en wat bijbesteld moet worden.",
        ),
        (
            AgentProduct::Hr,
            "Mensen",
            "Vraag naar personeelsdossiers: wie afwezig is, hoeveel verlof resteert, en wat een brief moet zeggen.",
        ),
        (
            AgentProduct::Insights,
            "Inzichten",
            "Vraag wat de cijfers zeggen, waarom een getal veranderde, en wat een rapport moet tonen.",
        ),
        (
            AgentProduct::Meet,
            "Meet",
            "Vraag wat een vergadering besloot, wat er gezegd is, en wat het u te doen geeft.",
        ),
        (
            AgentProduct::Sites,
            "Websites",
            "Vraag naar uw website: wat een pagina zegt, wat te publiceren, en hoe het in een andere taal leest.",
        ),
        (
            AgentProduct::Workspace,
            "alo",
            "Stel elke vraag over de hele werkruimte — het vindt de juiste agent en laat ze samenwerken.",
        ),
    ],
};

/// The agent words for a language tag, falling back to the default table.
/// The primary subtag decides, so `fr-BE` and `fr` get the same words.
#[must_use]
pub fn agent_words_for(tag: &str) -> &'static AgentWordsTable {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR,
        "nl" => &NL,
        _ => &EN,
    }
}

/// The agents a tenant is seeded with, in the caller's language.
#[must_use]
pub fn agent_seed_for(tag: &str) -> AgentSeed {
    let table = agent_words_for(tag);
    AgentSeed {
        agents: table
            .agents
            .iter()
            .map(|&(product, name, description)| AgentWords {
                product,
                name: name.to_owned(),
                description: description.to_owned(),
            })
            .collect(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::ALL_AGENT_PRODUCTS;

    /// Every table this file offers, so a language added later is checked by
    /// every test below without any of them being edited.
    const TABLES: [&AgentWordsTable; 3] = [&EN, &FR, &NL];

    #[test]
    fn every_language_names_every_product_exactly_once() {
        for table in TABLES {
            for product in ALL_AGENT_PRODUCTS {
                let named = table
                    .agents
                    .iter()
                    .filter(|(p, _, _)| *p == product)
                    .count();
                assert_eq!(named, 1, "{}: {product} appears {named} times", table.lang);
            }
        }
    }

    #[test]
    fn every_language_writes_a_name_and_a_line_for_each() {
        for table in TABLES {
            for (product, name, description) in &table.agents {
                assert!(
                    !name.trim().is_empty(),
                    "{}: {product} has no name — the store refuses a half seed",
                    table.lang
                );
                assert!(
                    !description.trim().is_empty(),
                    "{}: {product} has no description, so its empty state says nothing",
                    table.lang
                );
            }
        }
    }

    #[test]
    fn two_agents_never_share_a_name_in_one_language() {
        // Two agents with the same name is not a constraint violation — the
        // handles differ — but it is a roster a person cannot read.
        for table in TABLES {
            let mut names: Vec<&str> = table.agents.iter().map(|(_, name, _)| *name).collect();
            let total = names.len();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), total, "{} repeats a name", table.lang);
        }
    }

    #[test]
    fn a_seed_this_file_produces_is_one_the_store_accepts() {
        for table in TABLES {
            let seed = agent_seed_for(table.lang);
            assert_eq!(
                seed.agents.len(),
                ALL_AGENT_PRODUCTS.len(),
                "{}",
                table.lang
            );
        }
    }

    #[test]
    fn a_language_we_do_not_have_gets_the_default_rather_than_a_refusal() {
        for tag in ["", "de", "pt-BR", "xx", "ZZ_zz"] {
            assert_eq!(agent_words_for(tag).lang, "en", "{tag}");
        }
    }

    #[test]
    fn the_primary_subtag_decides() {
        assert_eq!(agent_words_for("fr-BE").lang, "fr");
        assert_eq!(agent_words_for("nl_NL").lang, "nl");
        assert_eq!(agent_words_for("FR").lang, "fr");
    }
}
