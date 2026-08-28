//! One link for this area's integration tests. Every file under
//! `tests/` used to be its own crate root, so every one linked its
//! own binary against the whole crate and a crate change relinked
//! them all (~40 minutes). This root names the same files as modules
//! of one binary; the files, their names and their tests are unchanged.

mod common;

mod site_analytics_tenancy;
mod site_attribution_tenancy;
mod site_availability_seam;
mod site_bookings_public;
mod site_bookings_tenancy;
mod site_catalog_publish;
mod site_catalog_tenancy;
mod site_chat_actions_tenancy;
mod site_chat_limits_tenancy;
mod site_collection_publish;
mod site_collections_tenancy;
mod site_conversion_tenancy;
mod site_domain_purchases;
mod site_domains_tenancy;
mod site_editor_grants;
mod site_generation_tenancy;
mod site_grounding_tenancy;
mod site_heatmap_tenancy;
mod site_image_presentation;
mod site_orders;
mod site_page_protection_tenancy;
mod site_palette;
mod site_posts_tenancy;
mod site_public_lead_capture;
mod site_public_shop;
mod site_public_stock;
mod site_publish_schedule_tenancy;
mod site_registrar_fixture;
mod site_sections;
mod site_templates;
mod site_ticket_holds;
mod site_ticket_orders;
mod site_translation_tenancy;
mod site_versions_tenancy;
