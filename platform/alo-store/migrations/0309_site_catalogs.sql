-- alo Sites catalogs: the tenant's own list of things a site shows — dishes,
-- rooms, services, courses. Unlike a site collection (0171), which is a live
-- binding to a table in alo Base, a catalog IS the record: the rows live here
-- and are edited here, and importing from Base copies once rather than binding
-- forever. Prices are integer minor units of the catalog's own currency; there
-- is no floating point anywhere on this path.

CREATE TABLE site_catalogs (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id    TEXT NOT NULL,
    id         TEXT NOT NULL,
    name       TEXT NOT NULL,
    currency   TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_catalogs_currency_iso CHECK (currency ~ '^[A-Z]{3}$')
);

CREATE INDEX site_catalogs_by_site
    ON site_catalogs (tenant_id, site_id, created_at, id);

-- One grouping inside a catalog (a menu course, a room type, a service family).
-- `slug` is the stable public handle a catalog section filters by and the
-- rendered page anchors on, so it is unique per catalog.
CREATE TABLE site_catalog_categories (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    catalog_id TEXT NOT NULL,
    id         TEXT NOT NULL,
    name       TEXT NOT NULL,
    slug       TEXT NOT NULL,
    position   INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, catalog_id)
        REFERENCES site_catalogs(tenant_id, id) ON DELETE CASCADE,
    UNIQUE (tenant_id, catalog_id, slug)
);

CREATE INDEX site_catalog_categories_by_catalog
    ON site_catalog_categories (tenant_id, catalog_id, position, created_at, id);

-- One thing on offer. `category_id` deliberately carries no foreign key: a
-- deleted category must leave its items standing (uncategorised), and a
-- composite ON DELETE SET NULL would null `tenant_id` with it. The store
-- clears the reference inside the delete transaction instead, and every write
-- proves the category belongs to the same catalog.
--
-- `source_key` is the Base record an import copied this row from. It exists so
-- a second import updates the row it already created instead of duplicating it;
-- rows created by hand carry NULL and are never touched by an import.
CREATE TABLE site_catalog_items (
    tenant_id     TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    catalog_id    TEXT NOT NULL,
    id            TEXT NOT NULL,
    category_id   TEXT,
    name          TEXT NOT NULL,
    slug          TEXT NOT NULL,
    description   TEXT,
    price_cents   BIGINT,
    price_note    TEXT,
    image_blob_id TEXT,
    availability  TEXT NOT NULL DEFAULT 'available',
    position      INTEGER NOT NULL DEFAULT 0,
    source_key    TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, catalog_id)
        REFERENCES site_catalogs(tenant_id, id) ON DELETE CASCADE,
    UNIQUE (tenant_id, catalog_id, slug),
    CONSTRAINT site_catalog_items_price_non_negative
        CHECK (price_cents IS NULL OR price_cents >= 0),
    CONSTRAINT site_catalog_items_availability
        CHECK (availability IN ('available', 'sold_out', 'hidden'))
);

CREATE INDEX site_catalog_items_by_catalog
    ON site_catalog_items (tenant_id, catalog_id, position, created_at, id);

CREATE UNIQUE INDEX site_catalog_items_by_source
    ON site_catalog_items (tenant_id, catalog_id, source_key)
    WHERE source_key IS NOT NULL;
