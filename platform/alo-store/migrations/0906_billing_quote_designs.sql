-- The designed presentation of a quotation (the quotation studio): the content
-- blocks a salesperson lays out around the price table — headings, paragraphs,
-- lists, pictures, tables, dividers — plus the colours and the column choices.
-- Until now this lived only in the browser that composed it, so the printed
-- document the customer received carried none of it. One row per quote, the
-- whole design as one JSON document: the web client owns its shape and the
-- server reads only the parts it prints, so a new block kind is a client
-- release, not a migration.
--
-- Composite reference: the design belongs to THIS tenant's quote, and goes
-- with it when the quote is deleted.
CREATE TABLE billing_quote_designs (
    tenant_id   TEXT NOT NULL,
    quote_id    TEXT NOT NULL,
    design      JSONB NOT NULL,
    updated_by  TEXT NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, quote_id),
    FOREIGN KEY (tenant_id, quote_id)
        REFERENCES billing_quotes (tenant_id, id) ON DELETE CASCADE
);
