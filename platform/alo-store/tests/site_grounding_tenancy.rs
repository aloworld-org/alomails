//! The site assistant's grounding boundary (ADR 0040 §1, item S3.02a).
//!
//! Law 1 discipline for the corpus: tenant isolation is tested, not assumed —
//! and so is the publish boundary. The load-bearing property is that **no
//! unpublished string can ever be retrieved**: drafts, never-published pages,
//! scheduled-but-unrun publishes, rolled-back past versions, draft posts, and
//! documents nobody added to the Public knowledge collection must all be
//! absent from the corpus by construction.

#![allow(clippy::unwrap_used)]

use crate::common;

use alo_store::{
    DriveLocation, GroundingCitation, GroundingDocument, NewDriveFile, NewSitePost,
    SITE_KNOWLEDGE_MAX_SOURCES, StoreError,
};
use bytes::Bytes;
use time::{Duration, OffsetDateTime};

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected Conflict, got {other:?}"),
    }
}

/// A unique dns-safe subdomain per test run.
fn subdomain(tag: &str) -> String {
    format!(
        "{tag}{}",
        alo_store::SiteId::generate()
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(16)
            .collect::<String>()
            .to_ascii_lowercase()
    )
}

/// A minimal BlockNote document whose only text run is `text`.
fn doc_bytes(text: &str) -> Bytes {
    Bytes::from(
        serde_json::json!([
            {"type": "paragraph", "content": [{"type": "text", "text": text, "styles": {}}]}
        ])
        .to_string(),
    )
}

/// Everything the corpus would let retrieval see, flattened for `contains`.
fn corpus_text(corpus: &[GroundingDocument]) -> String {
    corpus
        .iter()
        .map(|doc| format!("{}\n{}", doc.title, doc.text))
        .collect::<Vec<_>>()
        .join("\n")
}

fn hero_faq_sections(hero: &str, answer: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "sections": [
            {"type": "hero", "heading": hero},
            {"type": "faq", "items": [
                {"question": "Which areas do you serve?", "answer": answer}
            ]}
        ]
    })
}

/// Creates a doc node in Drive whose body is one BlockNote text run.
async fn create_doc(
    account: &alo_store::AccountStore,
    name: &str,
    text: &str,
) -> alo_store::DriveNodeId {
    let bytes = doc_bytes(text);
    let size = i64::try_from(bytes.len()).unwrap();
    let blob = account
        .put_blob(bytes, Some("application/json"))
        .await
        .unwrap();
    account
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: name.to_owned(),
                blob_id: blob.as_str().to_owned(),
                size,
                kind: Some("doc".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn no_unpublished_string_is_ever_in_the_corpus() {
    let store = common::test_store().await;
    let (account, _, _) = common::fresh_account(&store, "grounding-publish").await;

    let site = account
        .create_site("Boiler & Sons", &subdomain("grounding"))
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    account
        .set_page_sections(
            &site,
            &home,
            hero_faq_sections(
                "PUBLISHED_HERO_FACT boiler repair in Ghent",
                "PUBLISHED_FAQ_FACT all of East Flanders",
            ),
        )
        .await
        .unwrap();

    // Not live yet: nothing is on the internet, so the corpus is empty —
    // the draft heading is unretrievable even though it is saved.
    assert!(
        account
            .site_grounding_corpus(&site)
            .await
            .unwrap()
            .is_empty(),
        "a site that is not live must have an empty corpus"
    );

    account.publish_site(&site).await.unwrap();
    let corpus = account.site_grounding_corpus(&site).await.unwrap();
    let text = corpus_text(&corpus);
    assert!(text.contains("PUBLISHED_HERO_FACT"));
    assert!(text.contains("PUBLISHED_FAQ_FACT"));
    assert!(
        corpus.iter().any(|doc| matches!(
            &doc.citation,
            GroundingCitation::Page { slug, .. } if slug.is_empty()
        )),
        "the home page must be citable"
    );

    // Edit the draft, add a brand-new draft page, and schedule (but never
    // run) a publish. None of it may surface.
    account
        .set_page_sections(
            &site,
            &home,
            hero_faq_sections(
                "PUBLISHED_HERO_FACT boiler repair in Ghent",
                "DRAFT_ONLY_SECRET we are being acquired",
            ),
        )
        .await
        .unwrap();
    let hidden = account
        .create_site_page(&site, "Internal", "internal", false)
        .await
        .unwrap();
    account
        .set_page_sections(
            &site,
            &hidden,
            hero_faq_sections("DRAFT_PAGE_SECRET unannounced product", "Not yet public"),
        )
        .await
        .unwrap();
    account
        .schedule_site_publish(&site, OffsetDateTime::now_utc() + Duration::hours(1))
        .await
        .unwrap();
    let text = corpus_text(&account.site_grounding_corpus(&site).await.unwrap());
    assert!(
        text.contains("PUBLISHED_FAQ_FACT"),
        "the served set still grounds"
    );
    assert!(
        !text.contains("DRAFT_ONLY_SECRET"),
        "an edited draft must not be retrievable"
    );
    assert!(
        !text.contains("DRAFT_PAGE_SECRET"),
        "a scheduled-but-unrun publish must not leak its draft"
    );

    // Republishing makes the drafts the published set — and only then.
    account.publish_site(&site).await.unwrap();
    let text = corpus_text(&account.site_grounding_corpus(&site).await.unwrap());
    assert!(text.contains("DRAFT_ONLY_SECRET"));
    assert!(text.contains("DRAFT_PAGE_SECRET"));

    // A string removed and republished lives only in a PAST publish — past
    // versions are retained for rollback but must not ground answers.
    account
        .set_page_sections(
            &site,
            &home,
            hero_faq_sections(
                "PUBLISHED_HERO_FACT boiler repair in Ghent",
                "The old answer is gone",
            ),
        )
        .await
        .unwrap();
    account.publish_site(&site).await.unwrap();
    let text = corpus_text(&account.site_grounding_corpus(&site).await.unwrap());
    assert!(
        !text.contains("DRAFT_ONLY_SECRET"),
        "a rolled-past string must leave the corpus with the republish"
    );

    // Unpublishing empties the corpus entirely.
    account.unpublish_site(&site).await.unwrap();
    assert!(
        account
            .site_grounding_corpus(&site)
            .await
            .unwrap()
            .is_empty(),
        "an unpublished site must have an empty corpus"
    );
}

#[tokio::test]
async fn posts_ground_only_while_published() {
    let store = common::test_store().await;
    let (account, _, _) = common::fresh_account(&store, "grounding-posts").await;

    let site = account
        .create_site("The journal", &subdomain("posts"))
        .await
        .unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    account
        .set_page_sections(&site, &home, hero_faq_sections("Welcome", "Yes"))
        .await
        .unwrap();
    account.publish_site(&site).await.unwrap();

    let doc = create_doc(&account, "Launch story", "DRAFT_POST_SECRET ships in June").await;
    let post = account
        .create_site_post(
            &site,
            &NewSitePost {
                doc_node_id: &doc,
                slug: "launch",
                title: "The launch",
                excerpt: "POST_EXCERPT_FACT",
                cover_blob_id: None,
            },
        )
        .await
        .unwrap();

    let text = corpus_text(&account.site_grounding_corpus(&site).await.unwrap());
    assert!(
        !text.contains("DRAFT_POST_SECRET") && !text.contains("POST_EXCERPT_FACT"),
        "a draft post must not be retrievable"
    );

    account.publish_site_post(&site, &post).await.unwrap();
    let corpus = account.site_grounding_corpus(&site).await.unwrap();
    let text = corpus_text(&corpus);
    assert!(text.contains("DRAFT_POST_SECRET ships in June"));
    assert!(text.contains("POST_EXCERPT_FACT"));
    assert!(
        corpus.iter().any(|doc| matches!(
            &doc.citation,
            GroundingCitation::Post { slug } if slug == "launch"
        )),
        "a published post must be citable by slug"
    );

    account.unpublish_site_post(&site, &post).await.unwrap();
    let text = corpus_text(&account.site_grounding_corpus(&site).await.unwrap());
    assert!(
        !text.contains("DRAFT_POST_SECRET"),
        "an unpublished post must leave the corpus"
    );
}

#[tokio::test]
async fn knowledge_is_a_deliberate_act_and_tenant_scoped() {
    let store = common::test_store().await;
    let (a, _, _) = common::fresh_account(&store, "grounding-know-a").await;
    let (b, _, _) = common::fresh_account(&store, "grounding-know-b").await;

    let a_site = a
        .create_site("A consulting", &subdomain("knowa"))
        .await
        .unwrap();
    let home = a.create_site_page(&a_site, "Home", "", true).await.unwrap();
    a.set_page_sections(&a_site, &home, hero_faq_sections("Welcome", "Yes"))
        .await
        .unwrap();
    a.publish_site(&a_site).await.unwrap();
    let b_site = b
        .create_site("B private", &subdomain("knowb"))
        .await
        .unwrap();

    let published_doc = create_doc(&a, "Price list", "KNOWLEDGE_FACT day rate 900 euro").await;
    let _private_doc = create_doc(&a, "Internal notes", "NEVER_ADDED_SECRET margins").await;
    let b_doc = create_doc(&b, "B document", "B_TENANT_SECRET").await;

    let source = a
        .add_site_knowledge_source(&a_site, &published_doc)
        .await
        .unwrap();

    // Publishing to the assistant is per-document: the added document
    // grounds, the sibling that was never added does not exist here.
    let corpus = a.site_grounding_corpus(&a_site).await.unwrap();
    let text = corpus_text(&corpus);
    assert!(text.contains("KNOWLEDGE_FACT day rate 900 euro"));
    assert!(
        !text.contains("NEVER_ADDED_SECRET"),
        "a document nobody added must never be retrievable"
    );
    assert!(corpus.iter().any(|doc| matches!(
        &doc.citation,
        GroundingCitation::Knowledge { source_id } if *source_id == source
    )));

    // The same document cannot be added twice.
    assert_conflict(a.add_site_knowledge_source(&a_site, &published_doc).await);

    // Cross-tenant walls, in every direction.
    assert_not_found(b.site_grounding_corpus(&a_site).await);
    assert_not_found(b.site_knowledge_sources(&a_site).await);
    assert_not_found(b.add_site_knowledge_source(&a_site, &b_doc).await);
    assert_not_found(b.add_site_knowledge_source(&b_site, &published_doc).await);
    assert_not_found(b.remove_site_knowledge_source(&a_site, &source).await);
    let text = corpus_text(&a.site_grounding_corpus(&a_site).await.unwrap());
    assert!(!text.contains("B_TENANT_SECRET"));

    // Trashing the document silently withdraws it (fail-closed) while the
    // binding stays visible and removable.
    a.drive_trash_node(&published_doc).await.unwrap();
    let text = corpus_text(&a.site_grounding_corpus(&a_site).await.unwrap());
    assert!(
        !text.contains("KNOWLEDGE_FACT"),
        "a trashed document must stop grounding immediately"
    );
    let listed = a.site_knowledge_sources(&a_site).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].trashed);
    a.remove_site_knowledge_source(&a_site, &source)
        .await
        .unwrap();
    assert!(a.site_knowledge_sources(&a_site).await.unwrap().is_empty());
}

#[tokio::test]
async fn only_readable_documents_enter_the_collection_and_the_cap_holds() {
    let store = common::test_store().await;
    let (account, _, _) = common::fresh_account(&store, "grounding-cap").await;
    let site = account
        .create_site("Capped", &subdomain("cap"))
        .await
        .unwrap();

    // An image is not a readable source.
    let image = account
        .drive_create_file(
            &DriveLocation::Personal,
            None,
            &NewDriveFile {
                name: "Logo".to_owned(),
                blob_id: "grounding-logo".to_owned(),
                content_type: Some("image/png".to_owned()),
                ..NewDriveFile::default()
            },
        )
        .await
        .unwrap();
    assert_conflict(account.add_site_knowledge_source(&site, &image).await);

    // A folder is not a source at all.
    let folder = account
        .drive_create_folder(&DriveLocation::Personal, None, "Docs")
        .await
        .unwrap();
    assert_conflict(account.add_site_knowledge_source(&site, &folder).await);

    // The collection is bounded.
    for n in 0..SITE_KNOWLEDGE_MAX_SOURCES {
        let doc = create_doc(&account, &format!("Doc {n}"), &format!("fact {n}")).await;
        account
            .add_site_knowledge_source(&site, &doc)
            .await
            .unwrap();
    }
    let overflow = create_doc(&account, "One too many", "the last straw").await;
    assert_conflict(account.add_site_knowledge_source(&site, &overflow).await);
}

/// The public door (S3.02e): the visitor assistant reads the corpus through
/// `SitePublicStore`, and it must be byte-identical to the authenticated
/// assembly — one set of rules, two doors — while a resolved host can only
/// ever read its own tenant's corpus. The tenant's AI backend follows the
/// same scoping: the resolved site answers with its own tenant's default
/// provider or nothing.
#[tokio::test]
async fn the_public_door_reads_the_same_corpus_and_only_its_own() {
    let (store, blobs) = common::test_store_with_blobs().await;
    let public = alo_store::SitePublicStore::new(
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&common::database_url())
            .await
            .unwrap(),
        blobs,
    );

    let (a, _, _) = common::fresh_account(&store, "grounding-public-a").await;
    let (b, _, _) = common::fresh_account(&store, "grounding-public-b").await;

    let a_sub = subdomain("puba");
    let a_site = a.create_site("Bakery A", &a_sub).await.unwrap();
    let a_home = a.create_site_page(&a_site, "Home", "", true).await.unwrap();
    a.set_page_sections(
        &a_site,
        &a_home,
        hero_faq_sections("A_PAGE_FACT rye bread daily", "A_FAQ_FACT since 1998"),
    )
    .await
    .unwrap();
    a.publish_site(&a_site).await.unwrap();
    let a_doc = create_doc(&a, "Launch story", "A_POST_FACT sourdough week").await;
    let a_post = a
        .create_site_post(
            &a_site,
            &NewSitePost {
                doc_node_id: &a_doc,
                slug: "launch",
                title: "The launch",
                excerpt: "A launch excerpt",
                cover_blob_id: None,
            },
        )
        .await
        .unwrap();
    a.publish_site_post(&a_site, &a_post).await.unwrap();
    let a_knowledge = create_doc(&a, "Price list", "A_KNOWLEDGE_FACT day rate 900").await;
    a.add_site_knowledge_source(&a_site, &a_knowledge)
        .await
        .unwrap();

    let b_sub = subdomain("pubb");
    let b_site = b.create_site("Bakery B", &b_sub).await.unwrap();
    let b_home = b.create_site_page(&b_site, "Home", "", true).await.unwrap();
    b.set_page_sections(
        &b_site,
        &b_home,
        hero_faq_sections("B_PAGE_FACT spelt loaves", "B_FAQ_FACT since 2001"),
    )
    .await
    .unwrap();
    b.publish_site(&b_site).await.unwrap();

    // The public assembly is the authenticated assembly, verbatim.
    let resolved_a = public.resolve_published(&a_sub).await.unwrap().unwrap();
    let via_public = public.site_grounding_corpus(&resolved_a).await.unwrap();
    let via_account = a.site_grounding_corpus(&a_site).await.unwrap();
    assert_eq!(via_public, via_account, "the two doors must never fork");
    let text = corpus_text(&via_public);
    assert!(text.contains("A_PAGE_FACT"));
    assert!(text.contains("A_POST_FACT"));
    assert!(text.contains("A_KNOWLEDGE_FACT"));

    // A resolved host reads its own tenant's strings and nobody else's.
    assert!(!text.contains("B_PAGE_FACT"));
    let resolved_b = public.resolve_published(&b_sub).await.unwrap().unwrap();
    let b_text = corpus_text(&public.site_grounding_corpus(&resolved_b).await.unwrap());
    assert!(b_text.contains("B_PAGE_FACT"));
    assert!(!b_text.contains("A_PAGE_FACT") && !b_text.contains("A_KNOWLEDGE_FACT"));

    // The AI backend follows the same wall: no provider → None; tenant A's
    // enabled default → A's config (first listed model); tenant B still None.
    assert!(
        public
            .tenant_ai_config(&resolved_a)
            .await
            .unwrap()
            .is_none()
    );
    // Provider ids are global primary keys in the shared test database, so
    // the id must be fresh per run — a repeated fixed id would still belong
    // to an earlier run's tenant and the upsert would silently touch nothing.
    let provider = format!("prov-{}", alo_store::SiteId::generate().as_str());
    a.upsert_ai_provider(
        &provider,
        "openai_compatible",
        "Test backend",
        "http://127.0.0.1:1",
        "model-one, model-two",
        Some("secret-key"),
        true,
    )
    .await
    .unwrap();
    a.set_default_ai_provider(&provider).await.unwrap();
    let config = public.tenant_ai_config(&resolved_a).await.unwrap().unwrap();
    assert_eq!(config.base_url, "http://127.0.0.1:1");
    assert_eq!(config.model, "model-one");
    assert_eq!(config.api_key.as_deref(), Some("secret-key"));
    assert!(config.enabled);
    assert!(
        public
            .tenant_ai_config(&resolved_b)
            .await
            .unwrap()
            .is_none(),
        "tenant B must never read tenant A's backend"
    );
}
