//! Explicit, deterministic Billing demo corpus for local development and tests.
//!
//! Nothing calls this module during server startup. A caller must first mint a
//! [`DemoEnvironment`] by proving both the deployment mode and database target
//! are non-production, then choose an account door explicitly.

use std::str::FromStr;

use sqlx::postgres::PgConnectOptions;
use time::{Date, Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_sequence::{
    self, INVOICE_NUMBER_PREFIX, INVOICE_SEQUENCE_KIND, QUOTE_NUMBER_PREFIX, QUOTE_SEQUENCE_KIND,
};
use crate::billing_totals::{self, LineFigures};
use crate::error::{Result, StoreError};

const VERSION: i32 = 1;
const PREFIX: &str = "demo-billing-v1";

/// Runtime proof that demo writes cannot target production.
#[derive(Debug)]
pub struct DemoEnvironment {
    _private: (),
}

impl DemoEnvironment {
    /// Validates the deployment label and PostgreSQL target.
    pub fn validate(database_url: &str, deployment: &str) -> std::result::Result<Self, String> {
        let options = PgConnectOptions::from_str(database_url)
            .map_err(|_| "Billing demo data requires a valid PostgreSQL URL".to_owned())?;
        if !matches!(options.get_host(), "127.0.0.1" | "localhost" | "::1") {
            return Err("Billing demo data requires a loopback PostgreSQL host".to_owned());
        }
        let database = options.get_database().unwrap_or("");
        match deployment {
            "development" if database == "alo" => Ok(Self { _private: () }),
            "test" if database != "alo" && database != "ficina" && !database.is_empty() => {
                Ok(Self { _private: () })
            }
            "production" => Err("Billing demo data is disabled in production".to_owned()),
            "development" => Err(
                "development Billing demo data may only target the local database named alo"
                    .to_owned(),
            ),
            "test" => Err("tests may not seed a product database".to_owned()),
            _ => Err("ALO_ENV must be development or test for Billing demo data".to_owned()),
        }
    }
}

/// Counts returned by seed/reset verification and printed by the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemoCounts {
    pub customers: i64,
    pub products: i64,
    pub price_connections: i64,
    pub quotes: i64,
    pub invoices: i64,
    pub schedules: i64,
    pub vat_source_invoices: i64,
    pub quote_product_links: i64,
    pub invoice_product_links: i64,
    pub schedule_product_links: i64,
}

fn id(kind: &str, index: usize) -> String {
    format!("{PREFIX}-{kind}-{index:03}")
}

fn contact_id(account: &AccountStore, index: usize) -> String {
    format!("{PREFIX}-{}-contact-{index:03}", account.tenant.as_str())
}

fn base() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_787_875_200).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

fn day(offset: i64) -> Date {
    (base() + Duration::days(offset)).date()
}
fn stamp(offset: i64) -> OffsetDateTime {
    base() + Duration::days(offset)
}

fn currency(index: usize) -> &'static str {
    ["EUR", "EUR", "EUR", "USD", "GBP", "CHF", "SEK"][index % 7]
}

fn fx_rate(code: &str) -> i64 {
    match code {
        "EUR" => 1_000_000,
        "USD" => 1_172_400,
        "GBP" => 864_200,
        "CHF" => 938_500,
        "SEK" => 11_083_000,
        _ => 1_000_000,
    }
}

fn fictional_vat_id(country: &str, index: usize) -> String {
    let value = index + 10_000_000;
    match country {
        "BE" => format!("BE0{:09}", value % 1_000_000_000),
        "NL" => format!("NL{:09}B01", value % 1_000_000_000),
        "FR" => format!("FRXX{:09}", value % 1_000_000_000),
        "DE" => format!("DE{:09}", value % 1_000_000_000),
        "LU" => format!("LU{:08}", value % 100_000_000),
        "AT" => format!("ATU{:08}", value % 100_000_000),
        "ES" => format!("ESX{:08}", value % 100_000_000),
        "IT" => format!("IT{:011}", value % 100_000_000_000),
        "IE" => format!("IE{:07}X", value % 10_000_000),
        "SE" => format!("SE{:012}", value % 1_000_000_000_000),
        _ => format!("{country}{value}"),
    }
}

fn product_values(index: usize) -> (String, &'static str, i64, i32) {
    let families = [
        "Advisory workshop",
        "Cloud workspace licence",
        "Security assessment",
        "Ergonomic desk lamp",
        "Recycled notebook set",
        "On-site installation",
        "Data migration package",
        "Support retainer",
        "Sensor gateway",
        "Training session",
    ];
    let units = [
        "hour", "month", "project", "piece", "box", "day", "GB", "quarter", "unit", "seat",
    ];
    let vats = [2100, 2100, 1900, 2000, 600, 2100, 0, 1200, 2500, 900];
    let name = if index == 98 {
        "Cross-border transformation programme with discovery, implementation, migration, training and an extended handover period".to_owned()
    } else {
        format!(
            "{} — edition {:02}",
            families[index % families.len()],
            index + 1
        )
    };
    (
        name,
        units[index % units.len()],
        if index == 0 {
            0
        } else if index == 99 {
            25_000_000
        } else {
            750 + i64::try_from(index).unwrap_or(0) * 437
        },
        vats[index % vats.len()],
    )
}

async fn table_count(account: &AccountStore, table: &str) -> Result<i64> {
    sqlx::query_scalar::<_, i64>(&format!(
        "SELECT count(*)::bigint FROM {table} WHERE tenant_id=$1 AND id LIKE $2"
    ))
    .bind(account.tenant.as_str())
    .bind(format!("{PREFIX}-%"))
    .fetch_one(&account.pool)
    .await
    .map_err(StoreError::Db)
}

async fn counts(account: &AccountStore) -> Result<DemoCounts> {
    let vat_source_invoices: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM billing_invoices WHERE tenant_id=$1 AND id LIKE $2 AND status IN ('issued','paid')")
        .bind(account.tenant.as_str()).bind(format!("{PREFIX}-%"))
        .fetch_one(&account.pool).await.map_err(StoreError::Db)?;
    let quote_product_links: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM billing_quote_lines WHERE tenant_id=$1 AND quote_id LIKE $2 AND product_id IS NOT NULL")
        .bind(account.tenant.as_str()).bind(format!("{PREFIX}-%"))
        .fetch_one(&account.pool).await.map_err(StoreError::Db)?;
    let invoice_product_links: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM billing_invoice_lines WHERE tenant_id=$1 AND invoice_id LIKE $2 AND source_product_id IS NOT NULL")
        .bind(account.tenant.as_str()).bind(format!("{PREFIX}-%"))
        .fetch_one(&account.pool).await.map_err(StoreError::Db)?;
    let schedule_product_links: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM billing_schedule_lines WHERE tenant_id=$1 AND schedule_id LIKE $2 AND source_product_id IS NOT NULL")
        .bind(account.tenant.as_str()).bind(format!("{PREFIX}-%"))
        .fetch_one(&account.pool).await.map_err(StoreError::Db)?;
    Ok(DemoCounts {
        customers: table_count(account, "billing_customers").await?,
        products: table_count(account, "billing_products").await?,
        price_connections: table_count(account, "billing_price_connections").await?,
        quotes: table_count(account, "billing_quotes").await?,
        invoices: table_count(account, "billing_invoices").await?,
        schedules: table_count(account, "billing_schedules").await?,
        vat_source_invoices,
        quote_product_links,
        invoice_product_links,
        schedule_product_links,
    })
}

impl AccountStore {
    /// Seeds the versioned corpus once for this explicit tenant/account.
    pub async fn seed_billing_demo(&self, _environment: &DemoEnvironment) -> Result<DemoCounts> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(self.tenant.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        let already: Option<i32> =
            sqlx::query_scalar("SELECT version FROM billing_demo_seeds WHERE tenant_id=$1")
                .bind(self.tenant.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
        if already == Some(VERSION) {
            // Earlier v1 corpora deliberately omitted every eleventh invoice
            // address. Every demo customer also owns a quote and invoice, so
            // those records exposed a delivery action that could never work.
            // Converge an existing corpus as well as a fresh one: `seed` is a
            // safe repair command, not only a duplicate-prevention check.
            for i in 0..100usize {
                sqlx::query(
                    "UPDATE billing_customers SET email=$1 WHERE tenant_id=$2 AND id=$3 AND email IS NULL",
                )
                .bind(format!("accounts+{:03}@customer.example", i + 1))
                .bind(self.tenant.as_str())
                .bind(id("customer", i + 1))
                .execute(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
            }
            tx.commit().await.map_err(StoreError::Db)?;
            return counts(self).await;
        }
        if already.is_some() {
            return Err(StoreError::Conflict(
                "Billing demo data was created by a newer seed version".to_owned(),
            ));
        }
        let countries = [
            ("BE", "Brussels", "Rue du Marché"),
            ("NL", "Utrecht", "Singel"),
            ("FR", "Lille", "Rue Nationale"),
            ("DE", "Cologne", "Rheinufer"),
            ("LU", "Luxembourg", "Rue des Jardins"),
            ("AT", "Graz", "Murplatz"),
            ("ES", "Valencia", "Carrer de la Pau"),
            ("IT", "Bologna", "Via Europa"),
            ("IE", "Cork", "Harbour Road"),
            ("SE", "Malmö", "Öresundsgatan"),
        ];
        let roots = [
            "Northstar",
            "Juniper",
            "Canal",
            "Alpine",
            "Copper",
            "Civic",
            "Linden",
            "Harbour",
            "Mosaic",
            "Verdant",
        ];
        let forms = [
            "Studio",
            "Logistics",
            "Engineering",
            "Foods",
            "Mobility",
            "Advisory",
            "Manufacturing",
            "Digital",
            "Healthcare",
            "Cooperative",
        ];
        let settings_inserted = sqlx::query("INSERT INTO billing_settings (tenant_id,legal_name,address_line1,address_line2,postal_code,city,country,vat_id,registration_no,email,phone,website,iban,bic,bank_name,account_holder,footer_note,updated_by,base_currency) VALUES ($1,'Alo Demo Works SRL','Fictional Avenue 42','Demo floor','1000','Brussels','BE','BE0123456749','BE-DEMO-2026','billing@demo-works.example','+32 2 555 20 26','https://demo-works.example','BE68539007547034','KREDBEBB','European Demo Bank','Alo Demo Works SRL','Thank you for supporting independent European business.',$2,'EUR') ON CONFLICT (tenant_id) DO NOTHING")
            .bind(self.tenant.as_str()).bind(self.user.as_str()).execute(&mut *tx).await.map_err(StoreError::Db)?.rows_affected() == 1;
        let mut fx_currencies = Vec::new();
        for code in ["USD", "GBP", "CHF", "SEK"] {
            let inserted = sqlx::query("INSERT INTO billing_fx_rates (tenant_id,currency,rate_date,rate_micro,source,updated_by) VALUES ($1,$2,$3,$4,'manual',$5) ON CONFLICT (tenant_id,currency,rate_date) DO NOTHING")
                .bind(self.tenant.as_str()).bind(code).bind(day(-730)).bind(fx_rate(code)).bind(self.user.as_str())
                .execute(&mut *tx).await.map_err(StoreError::Db)?.rows_affected() == 1;
            if inserted {
                fx_currencies.push(code.to_owned());
            }
        }

        for i in 0..100usize {
            let (country, city, street) = countries[i % countries.len()];
            let first_names = [
                "Sofia", "Noah", "Emma", "Lukas", "Mila", "Hugo", "Nora", "Elias", "Lina", "Arthur",
            ];
            let last_names = [
                "Peeters",
                "De Vries",
                "Martin",
                "Schneider",
                "Rossi",
                "Novak",
                "Dubois",
                "Jensen",
                "Silva",
                "Bauer",
            ];
            let name = format!(
                "{} {} {}",
                roots[i % roots.len()],
                forms[(i / 10) % forms.len()],
                i + 1
            );
            // Every customer is linked to a quote, invoice and recurring
            // schedule, so every one needs a routable-looking invoice address.
            // Optional-field coverage remains in VAT ids, contacts and address
            // lines without making the primary delivery journey a dead end.
            let email = Some(format!("accounts+{:03}@customer.example", i + 1));
            let vat = (i % 13 != 0).then(|| fictional_vat_id(country, i + 1));
            let contact = contact_id(self, i + 1);
            let contact_email = format!(
                "{}.{}+{:03}@contacts.example",
                first_names[i % 10].to_lowercase(),
                last_names[i % 10].to_lowercase().replace(' ', "-"),
                i + 1
            );
            let emails = serde_json::json!([{"kind":"work","value":contact_email}]);
            let phones = serde_json::json!([{"kind":"work","value":format!("+{} 555 {:03} {:04}", 30 + i % 20, i + 1, 1000 + i)}]);
            sqlx::query("INSERT INTO contacts (tenant_id,user_id,id,display_name,first_name,last_name,emails,phones,organization,job_title,notes,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
                .bind(self.tenant.as_str()).bind(self.user.as_str()).bind(&contact)
                .bind(format!("{} {}", first_names[i % 10], last_names[i % 10]))
                .bind(first_names[i % 10]).bind(last_names[i % 10]).bind(emails).bind(phones)
                .bind(&name).bind("Billing contact").bind("Fictional development contact")
                .bind(stamp(-i64::try_from(i * 5).unwrap_or(0))).bind(stamp(-i64::try_from(i % 17).unwrap_or(0)))
                .execute(&mut *tx).await.map_err(StoreError::Db)?;
            sqlx::query("INSERT INTO billing_customers (tenant_id,id,name,address_line1,address_line2,postal_code,city,country,vat_id,email,payment_terms_days,currency,contact_id,archived_at,created_by,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)")
                .bind(self.tenant.as_str()).bind(id("customer", i + 1)).bind(name)
                .bind(format!("{street} {}", 10 + i)).bind(if i % 7 == 0 { "Attn. finance team" } else { "" })
                .bind(format!("{:04}", 1000 + i)).bind(city).bind(country).bind(vat).bind(email)
                .bind([0, 14, 21, 30, 45, 60][i % 6]).bind(currency(i))
                .bind((i % 9 != 0).then_some(contact))
                .bind((i % 10 == 0).then(|| stamp(-30))).bind(self.user.as_str())
                .bind(stamp(-i64::try_from(i * 5).unwrap_or(0))).bind(stamp(-i64::try_from(i % 17).unwrap_or(0)))
                .execute(&mut *tx).await.map_err(StoreError::Db)?;
            let (product_name, unit, price, vat_rate) = product_values(i);
            sqlx::query("INSERT INTO billing_products (tenant_id,id,name,unit,unit_price_cents,vat_rate_bp,sku,barcode,stocked,purchase_price_cents,photo_node_id,default_supplier_id,archived_at,created_by,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,'',$8,$9,NULL,NULL,$10,$11,$12,$13)")
                .bind(self.tenant.as_str()).bind(id("product", i + 1)).bind(product_name).bind(unit).bind(price).bind(vat_rate)
                .bind(format!("DEMO-{:04}", i + 1)).bind(i % 3 == 0).bind(price.saturating_mul(55) / 100)
                .bind((i % 10 == 9).then(|| stamp(-60))).bind(self.user.as_str())
                .bind(stamp(-i64::try_from(i * 3).unwrap_or(0))).bind(stamp(-i64::try_from(i % 23).unwrap_or(0)))
                .execute(&mut *tx).await.map_err(StoreError::Db)?;
        }

        for i in 0..100usize {
            let direction = if i % 2 == 0 { "received" } else { "shared" };
            let health = ["connected", "connected", "attention", "paused", "expired"][i % 5];
            let cadence = if direction == "received" {
                ["hourly", "daily", "weekly", "manual"][i % 4]
            } else {
                ["live", "approval"][i % 2]
            };
            sqlx::query("INSERT INTO billing_price_connections (tenant_id,id,direction,company,catalogue,health,cadence,channel,changes_count,last_synced_at,expires_at,created_by,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
                .bind(self.tenant.as_str()).bind(id("connection", i + 1)).bind(direction)
                .bind(format!("{} Partner {:03}", roots[i % roots.len()], i + 1))
                .bind(format!("{} catalogue {}", forms[i % forms.len()], currency(i))).bind(health).bind(cadence)
                .bind(if i % 3 == 0 { "api" } else { "alo" })
                .bind(if health == "attention" { i32::try_from(i % 12 + 1).unwrap_or(1) } else { 0 })
                .bind(Some(stamp(-i64::try_from(i % 40).unwrap_or(0))))
                .bind(if health == "expired" { Some(stamp(-1)) } else if i % 4 == 0 { Some(stamp(120)) } else { None })
                .bind(self.user.as_str()).bind(stamp(-i64::try_from(i * 2).unwrap_or(0)))
                .bind(stamp(-i64::try_from(i % 31).unwrap_or(0)))
                .execute(&mut *tx).await.map_err(StoreError::Db)?;
            for product in 0..(3 + i % 5) {
                sqlx::query("INSERT INTO billing_price_connection_products (tenant_id,connection_id,product_id) VALUES ($1,$2,$3)")
                    .bind(self.tenant.as_str()).bind(id("connection", i + 1))
                    .bind(id("product", (i + product) % 100 + 1))
                    .execute(&mut *tx).await.map_err(StoreError::Db)?;
            }
        }

        for i in 0..100usize {
            let status = ["draft", "sent", "accepted", "declined", "expired"][i % 5];
            let sent = (status != "draft").then(|| day(-i64::try_from(i * 4).unwrap_or(0)));
            let number = if let Some(sent_date) = sent {
                let drawn = billing_sequence::draw_next(
                    &mut tx,
                    self.tenant.as_str(),
                    QUOTE_SEQUENCE_KIND,
                    sent_date.year(),
                )
                .await?;
                Some(billing_sequence::document_number(
                    QUOTE_NUMBER_PREFIX,
                    sent_date.year(),
                    drawn,
                ))
            } else {
                None
            };
            let valid_days = [14, 21, 30, 45, 60][i % 5];
            let valid = sent.map(|date| date + Duration::days(i64::from(valid_days)));
            let decided = ["accepted", "declined", "expired"]
                .contains(&status)
                .then(|| sent.map(|date| date + Duration::days(7)))
                .flatten();
            sqlx::query("INSERT INTO billing_quotes (tenant_id,id,customer_id,status,currency,number,sent_date,valid_until,valid_days,decided_date,reference,note,created_by,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)")
                .bind(self.tenant.as_str()).bind(id("quote", i + 1)).bind(id("customer", i % 100 + 1))
                .bind(status).bind(currency(i)).bind(number).bind(sent).bind(valid).bind(valid_days).bind(decided)
                .bind(if i % 3 == 0 { format!("RFQ-CUST-{:04}", i + 1) } else { String::new() })
                .bind(if i % 9 == 0 { "Includes a staged delivery plan and optional follow-up workshop." } else { "" })
                .bind(self.user.as_str()).bind(stamp(-i64::try_from(i * 4 + 2).unwrap_or(0)))
                .bind(stamp(-i64::try_from(i % 19).unwrap_or(0)))
                .execute(&mut *tx).await.map_err(StoreError::Db)?;
            for line in 0..(2 + i % 4) {
                let product = (i * 3 + line) % 100;
                let (name, unit, price, vat) = product_values(product);
                sqlx::query("INSERT INTO billing_quote_lines (tenant_id,quote_id,id,line_order,description,unit,qty_milli,unit_price_cents,vat_rate_bp,product_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
                    .bind(self.tenant.as_str()).bind(id("quote", i + 1))
                    .bind(format!("{}-line-{line:02}", id("quote", i + 1)))
                    .bind(i32::try_from(line).unwrap_or(0)).bind(name).bind(unit)
                    .bind(i64::try_from(line + 1).unwrap_or(1) * 1000).bind(price).bind(vat)
                    .bind(id("product", product + 1)).execute(&mut *tx).await.map_err(StoreError::Db)?;
            }
            if i % 4 == 0 {
                let design = serde_json::json!({"theme":"modern","colors":{"accent":"#e76f51"},"blocks":[
                    {"id":"heading-1","kind":"heading","level":1,"text":"A practical European partnership"},
                    {"id":"paragraph-1","kind":"paragraph","text":"This fictional proposal combines delivery, onboarding and support.","columns":2},
                    {"id":"list-1","kind":"list","ordered":false,"items":"Discovery\nDelivery\nHandover","columns":3},
                    {"id":"divider-1","kind":"divider","style":"solid","width":75},
                    {"id":"image-1","kind":"image","src":"/demo/billing/workspace.svg","caption":"Reusable demo product illustration","placement":"right"},
                    {"id":"table-1","kind":"table","columns":[{"id":"phase","label":"Phase"},{"id":"owner","label":"Owner"}],"rows":[{"id":"r1","cells":{"phase":"Pilot","owner":"Joint team"}}]},
                    {"id":"pricing-1","kind":"pricing","title":"Implementation","showSubtotal":true},
                    {"id":"pricing-2","kind":"pricing","title":"Ongoing service","showSubtotal":true}
                ]});
                sqlx::query("INSERT INTO billing_quote_designs (tenant_id,quote_id,design,updated_by,updated_at) VALUES ($1,$2,$3,$4,$5)")
                    .bind(self.tenant.as_str()).bind(id("quote", i + 1)).bind(design)
                    .bind(self.user.as_str()).bind(stamp(-i64::try_from(i % 11).unwrap_or(0)))
                    .execute(&mut *tx).await.map_err(StoreError::Db)?;
            }
        }

        for i in 0..120usize {
            let status = if i < 10 {
                "draft"
            } else if i < 20 {
                "void"
            } else if i < 45 {
                "paid"
            } else {
                "issued"
            };
            let issue = (status != "draft").then(|| day(-i64::try_from((i - 10) * 5).unwrap_or(0)));
            let number = if let Some(issue_date) = issue {
                let drawn = billing_sequence::draw_next(
                    &mut tx,
                    self.tenant.as_str(),
                    INVOICE_SEQUENCE_KIND,
                    issue_date.year(),
                )
                .await?;
                Some(billing_sequence::document_number(
                    INVOICE_NUMBER_PREFIX,
                    issue_date.year(),
                    drawn,
                ))
            } else {
                None
            };
            let terms = [0, 14, 21, 30, 45, 60][i % 6];
            let doc_currency = currency(i);
            let quote_id = (i < 20).then(|| id("quote", ((i * 5 + 2) % 100) + 1));
            sqlx::query("INSERT INTO billing_invoices (tenant_id,id,customer_id,status,currency,number,issue_date,due_date,payment_terms_days,is_credit_note,credits_invoice_id,reference,note,created_by,created_at,updated_at,quote_id,fx_base_currency,fx_rate_micro,fx_rate_date,schedule_id,schedule_due_date) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,false,NULL,$10,$11,$12,$13,$14,$15,$16,$17,$18,NULL,NULL)")
                .bind(self.tenant.as_str()).bind(id("invoice", i + 1)).bind(id("customer", i % 100 + 1))
                .bind(status).bind(doc_currency).bind(number).bind(issue)
                .bind(issue.map(|date| date + Duration::days(i64::from(terms)))).bind(terms)
                .bind(if i % 4 == 0 { format!("PO-CLIENT-{:05}", i + 1) } else { String::new() })
                .bind(if i % 8 == 0 { "Please quote the customer reference with payment." } else { "" })
                .bind(self.user.as_str()).bind(stamp(-i64::try_from(i * 3 + 1).unwrap_or(0)))
                .bind(stamp(-i64::try_from(i % 29).unwrap_or(0))).bind(quote_id)
                .bind(issue.map(|_| "EUR")).bind(issue.map(|_| fx_rate(doc_currency))).bind(issue)
                .execute(&mut *tx).await.map_err(StoreError::Db)?;
            let mut figures = Vec::new();
            let line_count = if i == 0 { 0 } else { 2 + i % 5 };
            for line in 0..line_count {
                let product = (i * 7 + line) % 100;
                let (name, unit, price, vat) = product_values(product);
                let quantity = i64::try_from(line + 1).unwrap_or(1) * 1000;
                figures.push(LineFigures {
                    qty_milli: quantity,
                    unit_price_cents: price,
                    vat_rate_bp: vat,
                });
                sqlx::query("INSERT INTO billing_invoice_lines (tenant_id,invoice_id,id,line_order,description,unit,qty_milli,unit_price_cents,vat_rate_bp,source_product_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
                    .bind(self.tenant.as_str()).bind(id("invoice", i + 1))
                    .bind(format!("{}-line-{line:02}", id("invoice", i + 1)))
                    .bind(i32::try_from(line).unwrap_or(0)).bind(name).bind(unit).bind(quantity).bind(price).bind(vat)
                    .bind(id("product", product + 1))
                    .execute(&mut *tx).await.map_err(StoreError::Db)?;
            }
            let gross = billing_totals::totals(&figures).gross_cents;
            if status == "paid" || (status == "issued" && i % 7 == 0) {
                let amount = if status == "paid" {
                    gross.max(1)
                } else {
                    (gross / 3).max(1)
                };
                sqlx::query("INSERT INTO billing_payments (tenant_id,id,invoice_id,paid_on,amount_cents,method,reference,created_by,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                    .bind(self.tenant.as_str()).bind(id("payment", i + 1)).bind(id("invoice", i + 1))
                    .bind(issue.unwrap_or(day(-1)) + Duration::days(3)).bind(amount)
                    .bind(["SEPA transfer", "card", "direct debit"][i % 3])
                    .bind(format!("BANK-DEMO-{:05}", i + 1)).bind(self.user.as_str())
                    .bind(stamp(-i64::try_from(i % 31).unwrap_or(0)))
                    .execute(&mut *tx).await.map_err(StoreError::Db)?;
            }
        }

        for i in 0..100usize {
            let start = day(-i64::try_from(i % 180).unwrap_or(0));
            let cadence = ["weekly", "monthly", "quarterly", "yearly"][i % 4];
            let ended = i % 10 == 8;
            let next = if ended {
                day(-2)
            } else {
                start + Duration::days(i64::try_from(i % 75 + 1).unwrap_or(1))
            };
            let end = if ended {
                Some(day(-3))
            } else if i % 6 == 0 {
                Some(day(180))
            } else {
                None
            };
            sqlx::query("INSERT INTO billing_schedules (tenant_id,id,customer_id,name,cadence,anchor_day,start_date,end_date,next_run_date,last_run_date,active,currency,payment_terms_days,reference,note,created_by,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)")
                .bind(self.tenant.as_str()).bind(id("schedule", i + 1)).bind(id("customer", i % 100 + 1))
                .bind(format!("{} recurring plan {:03}", roots[i % roots.len()], i + 1)).bind(cadence)
                .bind(i16::try_from(i % 28 + 1).unwrap_or(1)).bind(start).bind(end).bind(next)
                .bind((i % 3 == 0).then_some(start)).bind(i % 10 != 7).bind(currency(i))
                .bind([14, 30, 45, 60][i % 4])
                .bind(if i % 3 == 0 { format!("SUB-{:04}", i + 1) } else { String::new() })
                .bind("Draft for review; this schedule never issues automatically.").bind(self.user.as_str())
                .bind(stamp(-i64::try_from(i * 3).unwrap_or(0))).bind(stamp(-i64::try_from(i % 17).unwrap_or(0)))
                .execute(&mut *tx).await.map_err(StoreError::Db)?;
            for line in 0..2usize {
                let product = (i + line) % 100;
                let (name, unit, price, vat) = product_values(product);
                sqlx::query("INSERT INTO billing_schedule_lines (tenant_id,schedule_id,id,line_order,description,unit,qty_milli,unit_price_cents,vat_rate_bp,source_product_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
                    .bind(self.tenant.as_str()).bind(id("schedule", i + 1))
                    .bind(format!("{}-line-{line:02}", id("schedule", i + 1)))
                    .bind(i32::try_from(line).unwrap_or(0)).bind(name).bind(unit)
                    .bind(i64::try_from(line + 1).unwrap_or(1) * 1000).bind(price).bind(vat)
                    .bind(id("product", product + 1))
                    .execute(&mut *tx).await.map_err(StoreError::Db)?;
            }
        }

        for i in 0..100usize {
            let (entity, entity_id) = if i % 2 == 0 {
                ("billing.quote", id("quote", i + 1))
            } else {
                ("billing.invoice", id("invoice", i + 1))
            };
            sqlx::query("INSERT INTO audit_log (id,tenant_id,actor_user_id,action,target,entity_type,entity_id,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(crate::id::generate_token()).bind(self.tenant.as_str()).bind(self.user.as_str())
                .bind(if i % 3 == 0 { "billing.updated" } else { "billing.created" })
                .bind("development demo record").bind(entity).bind(entity_id)
                .bind(stamp(-i64::try_from(i % 40).unwrap_or(0)))
                .execute(&mut *tx).await.map_err(StoreError::Db)?;
        }
        sqlx::query(
            "INSERT INTO billing_demo_seeds (tenant_id,version,seeded_by,settings_inserted,fx_currencies) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(self.tenant.as_str())
        .bind(VERSION)
        .bind(self.user.as_str())
        .bind(settings_inserted)
        .bind(fx_currencies)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        counts(self).await
    }

    /// Removes only the versioned demo namespace from this tenant.
    pub async fn reset_billing_demo(&self, _environment: &DemoEnvironment) -> Result<DemoCounts> {
        let like = format!("{PREFIX}-%");
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let ownership: Option<(bool, Vec<String>)> = sqlx::query_as(
            "SELECT settings_inserted,fx_currencies FROM billing_demo_seeds WHERE tenant_id=$1",
        )
        .bind(self.tenant.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let (settings_inserted, fx_currencies) = ownership.unwrap_or_default();
        sqlx::query("DELETE FROM audit_log WHERE tenant_id=$1 AND entity_id LIKE $2")
            .bind(self.tenant.as_str())
            .bind(&like)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        sqlx::query("DELETE FROM billing_quote_designs WHERE tenant_id=$1 AND quote_id LIKE $2")
            .bind(self.tenant.as_str())
            .bind(&like)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        for table in [
            "billing_payments",
            "billing_invoices",
            "billing_quotes",
            "billing_schedules",
            "billing_price_connections",
            "billing_products",
            "billing_customers",
        ] {
            sqlx::query(&format!(
                "DELETE FROM {table} WHERE tenant_id=$1 AND id LIKE $2"
            ))
            .bind(self.tenant.as_str())
            .bind(&like)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        sqlx::query("DELETE FROM contacts WHERE tenant_id=$1 AND user_id=$2 AND id LIKE $3")
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(format!("{PREFIX}-%"))
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        sqlx::query("DELETE FROM billing_fx_rates WHERE tenant_id=$1 AND source='manual' AND updated_by=$2 AND rate_date=$3 AND currency=ANY($4::text[])")
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(day(-730))
        .bind(fx_currencies)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if settings_inserted {
            sqlx::query("DELETE FROM billing_settings WHERE tenant_id=$1 AND updated_by=$2 AND legal_name='Alo Demo Works SRL'")
                .bind(self.tenant.as_str())
                .bind(self.user.as_str())
                .execute(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
        }
        sqlx::query("DELETE FROM billing_demo_seeds WHERE tenant_id=$1")
            .bind(self.tenant.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        counts(self).await
    }
}
