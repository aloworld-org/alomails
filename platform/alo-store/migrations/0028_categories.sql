-- Per-message categories (Outlook-style colored labels): a user can tag a
-- message with one or more categories and later filter by them. Category
-- MEMBERSHIP already lives in message_keywords as a "$category_<id>" keyword
-- (arbitrary keywords are persisted there with no schema change); this table is
-- only the catalog — a category's display name and color — so chips render
-- consistently across every client and device. Deliberately off the hot message
-- path: a handful of rows per user.
CREATE TABLE categories (
    tenant_id  TEXT    NOT NULL,
    user_id    TEXT    NOT NULL,
    id         TEXT    NOT NULL PRIMARY KEY,
    name       TEXT    NOT NULL,
    color      TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    -- A user's category names are unique (case-sensitive) within their account.
    UNIQUE (tenant_id, user_id, name)
);

CREATE INDEX categories_owner ON categories (tenant_id, user_id, sort_order);
