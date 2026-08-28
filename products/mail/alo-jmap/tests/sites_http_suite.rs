//! One link for this area's integration tests. Every file under
//! `tests/` used to be its own crate root, so every one linked its
//! own binary against the whole crate and a crate change relinked
//! them all (~40 minutes). This root names the same files as modules
//! of one binary; the files, their names and their tests are unchanged.

mod common;

mod site_booking_notify;
mod site_chat_notify;
mod site_domain_registration;
mod site_editor_role_http;
mod site_inline_text;
mod site_notify;
mod site_palette_http;
mod site_protection_http;
mod site_schedule_http;
mod site_section_move;
mod site_versions_http;
mod sites_bookings_http;
mod sites_catalogs_http;
mod sites_domain_purchases_http;
mod sites_final_arc;
mod sites_generate_http;
mod sites_http;
mod sites_knowledge_http;
mod sites_orders_http;
mod sites_shop_config_http;
mod sites_shop_items_http;
mod sites_shop_settings_http;
