//! The grounding corpus for the site assistant (ADR 0040 §1, item S3.02a).
//!
//! The corpus is **only what is already on the internet**: the *published*
//! version of the site (its page snapshots and the collection rows frozen
//! with them), blog posts whose status is `published`, and the documents the
//! tenant deliberately added to the site's [Public knowledge
//! collection](crate::site_knowledge). Exclusion of drafts is by
//! construction, not by filtering: pages are read exclusively from the
//! immutable snapshot set the site's `published_publish_id` points at — the
//! same rows the public service serves — so an edited draft, a page that was
//! never published, a scheduled-but-unrun publish, and a rolled-back past
//! version are all equally unreachable from here. A site that is not live has
//! an **empty** corpus: when nothing is on the internet, the assistant may
//! read nothing, knowledge collection included.
//!
//! Two published surfaces stay out on purpose. Catalog prices and booking
//! availability are ADR 0040's "structured facts the tenant switches on one
//! by one" — they enter through their own explicit toggles in a later slice,
//! read through tool calls rather than retrieval, because *never a price the
//! model invented* means prices are answered from the catalog, not from
//! prose. And a `custom_code` section contributes only its heading and
//! accessible title, never its HTML/CSS/JS: code is not prose, and grounding
//! an answer in markup would let a script fragment masquerade as a fact.

use std::collections::BTreeMap;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::extract::extract_text;
use crate::id::{SiteId, SiteKnowledgeSourceId, SitePublishId};
use crate::site_model::{Section, SectionsEnvelope};

/// Where a grounding document came from — the citation every assistant answer
/// must carry (ADR 0040: an answer that cannot cite is not given).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroundingCitation {
    /// A published page snapshot; `slug` is empty for the home page.
    Page { slug: String, locale: String },
    /// A published blog post, served at `/blog/<slug>`.
    Post { slug: String },
    /// A document from the site's Public knowledge collection.
    Knowledge { source_id: SiteKnowledgeSourceId },
}

/// One retrievable unit of the corpus: a title, the text the internet can
/// already read, and the citation naming where it is public.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundingDocument {
    pub citation: GroundingCitation,
    pub title: String,
    pub text: String,
}

impl AccountStore {
    /// Assembles the assistant's complete grounding corpus for `site`. Order
    /// is stable: pages in navigation order, then published posts newest
    /// first, then knowledge sources oldest first.
    ///
    /// Returns an empty corpus when the site is not live — nothing published,
    /// nothing readable. A post or knowledge document whose Drive file is
    /// trashed, deleted, or unreadable contributes nothing (the same clean
    /// absence the public blog serves), never an error.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Conflict`] when a stored snapshot fails to parse (a
    /// write-gate invariant broken upstream); [`StoreError::Db`] on backend
    /// failure.
    pub async fn site_grounding_corpus(&self, site: &SiteId) -> Result<Vec<GroundingDocument>> {
        let published: Option<Option<String>> = sqlx::query_scalar(
            "SELECT published_publish_id FROM sites WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let Some(published) = published else {
            return Err(StoreError::NotFound);
        };
        let Some(publish_id) = published else {
            return Ok(Vec::new());
        };
        let publish_id = SitePublishId::new(publish_id);
        let mut corpus = Vec::new();
        self.ground_published_pages(site, &publish_id, &mut corpus)
            .await?;
        self.ground_published_posts(site, &mut corpus).await?;
        self.ground_knowledge_sources(site, &mut corpus).await?;
        Ok(corpus)
    }

    /// The published pages: every snapshot of the current publish, its typed
    /// section text plus the collection rows frozen with that same publish.
    async fn ground_published_pages(
        &self,
        site: &SiteId,
        publish: &SitePublishId,
        corpus: &mut Vec<GroundingDocument>,
    ) -> Result<()> {
        let collections: BTreeMap<String, Vec<String>> = self
            .site_publish_collection_snapshots(site, publish)
            .await?
            .into_iter()
            .map(|snapshot| {
                let mut parts = vec![snapshot.name];
                for item in snapshot.items {
                    parts.push(item.title);
                    parts.extend(item.summary);
                    parts.extend(item.body);
                }
                (snapshot.collection_id.as_str().to_owned(), parts)
            })
            .collect();
        for snapshot in self.site_publish_snapshots(site, publish).await? {
            let envelope = SectionsEnvelope::from_value(snapshot.sections).map_err(|error| {
                StoreError::Conflict(format!("published page snapshot is invalid: {error}"))
            })?;
            let mut parts = Vec::new();
            if let Some(description) = &snapshot.seo_description {
                push_text(&mut parts, description);
            }
            for section in &envelope.sections {
                parts.extend(section_text(section));
                if let Section::Collection(collection) = section
                    && let Some(items) = collections.get(collection.collection_id.as_str())
                {
                    parts.extend(items.iter().cloned());
                }
            }
            corpus.push(GroundingDocument {
                citation: GroundingCitation::Page {
                    slug: snapshot.slug,
                    locale: snapshot.locale,
                },
                title: snapshot.title,
                text: parts.join("\n"),
            });
        }
        Ok(())
    }

    /// The published blog posts, through the same gate the public service
    /// uses: status `published`, a non-trashed `doc` node, its current bytes.
    async fn ground_published_posts(
        &self,
        site: &SiteId,
        corpus: &mut Vec<GroundingDocument>,
    ) -> Result<()> {
        let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>, String)>(
            "SELECT p.slug, p.title, p.excerpt, d.kind, d.content_type, b.hash \
             FROM site_posts p \
             JOIN drive_nodes d ON d.tenant_id = p.tenant_id AND d.id = p.doc_node_id \
             JOIN blobs b ON b.tenant_id = d.tenant_id AND b.id = d.blob_id \
             WHERE p.tenant_id = $1 AND p.site_id = $2 \
               AND p.status = 'published' AND d.kind = 'doc' AND NOT d.trashed \
             ORDER BY p.published_at DESC, p.id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        for (slug, title, excerpt, kind, content_type, hash) in rows {
            let mut parts = Vec::new();
            push_text(&mut parts, &excerpt);
            if let Some(body) = self.extracted_blob_text(kind, content_type, &hash).await {
                parts.push(body);
            }
            corpus.push(GroundingDocument {
                citation: GroundingCitation::Post { slug },
                title,
                text: parts.join("\n"),
            });
        }
        Ok(())
    }

    /// The Public knowledge collection: each deliberately-added document's
    /// current text — the live-read the published blog set the precedent for,
    /// because the add was the deliberate act on exactly this document.
    async fn ground_knowledge_sources(
        &self,
        site: &SiteId,
        corpus: &mut Vec<GroundingDocument>,
    ) -> Result<()> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>)>(
            "SELECT k.id, d.name, d.kind, d.content_type, b.hash \
             FROM site_knowledge_sources k \
             JOIN drive_nodes d ON d.tenant_id = k.tenant_id AND d.id = k.doc_node_id \
             LEFT JOIN blobs b ON b.tenant_id = d.tenant_id AND b.id = d.blob_id \
             WHERE k.tenant_id = $1 AND k.site_id = $2 AND NOT d.trashed \
             ORDER BY k.added_at, k.id",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        for (source_id, name, kind, content_type, hash) in rows {
            let Some(hash) = hash else { continue };
            let Some(text) = self.extracted_blob_text(kind, content_type, &hash).await else {
                continue;
            };
            corpus.push(GroundingDocument {
                citation: GroundingCitation::Knowledge {
                    source_id: SiteKnowledgeSourceId::new(source_id),
                },
                title: name,
                text,
            });
        }
        Ok(())
    }

    /// Best-effort text extraction from one tenant blob, off the async
    /// runtime — the same discipline as Drive content indexing: a missing
    /// blob, a failed parse, or a parser panic is `None`, never an error.
    async fn extracted_blob_text(
        &self,
        kind: String,
        content_type: Option<String>,
        hash: &str,
    ) -> Option<String> {
        let bytes = self.blobs.get(self.tenant.as_str(), hash).await.ok()?;
        tokio::task::spawn_blocking(move || extract_text(&kind, content_type.as_deref(), &bytes))
            .await
            .ok()
            .flatten()
    }
}

/// The human-readable text of one typed section — every string the section
/// renders as visible (or screen-reader-announced) prose on the published
/// page, and nothing else: no hrefs, ids, icon tokens, layout values, or
/// custom code. Public so the retrieval slice (S3.02b) chunks the same text
/// this corpus is built from.
#[must_use]
pub fn section_text(section: &Section) -> Vec<String> {
    let mut parts = Vec::new();
    let out = &mut parts;
    match section {
        Section::Nav(section) => {
            for link in &section.links {
                push_text(out, &link.label);
            }
            if let Some(cta) = &section.cta {
                push_text(out, &cta.label);
            }
        }
        Section::Hero(section) => {
            push_text(out, &section.heading);
            push_opt(out, section.subheading.as_deref());
            push_alt(out, section.image.as_ref());
            push_opt(
                out,
                section.primary_cta.as_ref().map(|cta| cta.label.as_str()),
            );
            push_opt(
                out,
                section.secondary_cta.as_ref().map(|cta| cta.label.as_str()),
            );
        }
        Section::Features(section) => {
            push_opt(out, section.heading.as_deref());
            push_opt(out, section.intro.as_deref());
            for item in &section.items {
                push_text(out, &item.title);
                push_text(out, &item.body);
            }
        }
        Section::TextImage(section) => {
            push_opt(out, section.heading.as_deref());
            push_text(out, &section.body);
            push_alt(out, Some(&section.image));
        }
        Section::Gallery(section) => {
            push_opt(out, section.heading.as_deref());
            for image in &section.images {
                push_alt(out, Some(image));
            }
        }
        Section::Testimonials(section) => {
            push_opt(out, section.heading.as_deref());
            for item in &section.items {
                push_text(out, &item.quote);
                push_text(out, &item.author);
                push_opt(out, item.role.as_deref());
            }
        }
        Section::Pricing(section) => {
            push_opt(out, section.heading.as_deref());
            push_opt(out, section.intro.as_deref());
            for tier in &section.tiers {
                push_text(out, &tier.name);
                push_text(out, &tier.price);
                push_opt(out, tier.period.as_deref());
                push_opt(out, tier.description.as_deref());
                for feature in &tier.features {
                    push_text(out, feature);
                }
                push_opt(out, tier.cta.as_ref().map(|cta| cta.label.as_str()));
            }
        }
        Section::Team(section) => {
            push_opt(out, section.heading.as_deref());
            for member in &section.members {
                push_text(out, &member.name);
                push_opt(out, member.role.as_deref());
                push_opt(out, member.bio.as_deref());
                push_alt(out, member.photo.as_ref());
            }
        }
        Section::Faq(section) => {
            push_opt(out, section.heading.as_deref());
            for item in &section.items {
                push_text(out, &item.question);
                push_text(out, &item.answer);
            }
        }
        Section::Cta(section) => {
            push_text(out, &section.heading);
            push_opt(out, section.body.as_deref());
            push_text(out, &section.button.label);
        }
        Section::ContactForm(section) => {
            push_opt(out, section.heading.as_deref());
            push_opt(out, section.body.as_deref());
            push_opt(out, section.success_message.as_deref());
        }
        Section::Collection(section) => {
            // The frozen rows live with the publish, not in the section; the
            // corpus assembly appends them from the collection snapshot.
            push_opt(out, section.heading.as_deref());
        }
        Section::Catalog(section) => {
            // Prices enter only through their own ADR 0040 toggle, as
            // structured facts — never as retrievable prose.
            push_opt(out, section.heading.as_deref());
        }
        Section::Booking(section) => {
            // Availability likewise: a tool call in a later slice, not prose.
            push_opt(out, section.heading.as_deref());
        }
        Section::CustomCode(section) => {
            // Code is not prose: the heading and accessible title only.
            push_opt(out, section.heading.as_deref());
            push_text(out, &section.title);
        }
        Section::Footer(section) => {
            push_opt(out, section.text.as_deref());
            for link in &section.links {
                push_text(out, &link.label);
            }
        }
    }
    parts
}

fn push_text(out: &mut Vec<String>, value: &str) {
    let trimmed = value.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_owned());
    }
}

fn push_opt(out: &mut Vec<String>, value: Option<&str>) {
    if let Some(value) = value {
        push_text(out, value);
    }
}

/// Alt text is announced to screen-reader visitors, so it is published prose
/// — unless the image is decorative, in which case it says nothing.
fn push_alt(out: &mut Vec<String>, image: Option<&crate::site_model::SiteImage>) {
    if let Some(image) = image
        && !image.decorative
    {
        push_text(out, &image.alt);
    }
}
