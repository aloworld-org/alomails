-- How we know a person agreed to be mailed (alo Campaigns, ADR 0044 §2; queue
-- item C1.2).
--
-- ADR 0044 §2 decides that consent is "a record, not a checkbox": every
-- recipient carries the provenance of their consent — when, from which source,
-- from which address — and a campaign cannot be sent to somebody without one.
-- This table is that record.
--
-- Why a table of events rather than a column on a person:
--
-- "Did they agree" and "how do we know" are different questions, and only the
-- second survives a complaint. A boolean answers the first and destroys the
-- evidence for the second the moment it is overwritten. So every act of consent
-- is its own row and nothing here is ever updated: somebody who ticked a box on
-- a site form in March and re-confirmed in an imported list in June has two
-- rows, and a regulator asking about June gets June's statement rather than a
-- flag that says yes.
--
-- Keyed by ADDRESS, not by a customer or a deal. There is no list (ADR 0044's
-- central claim), so there is no row to hang consent off: the same person is a
-- billing customer, the contact on two deals and a form submitter at once, and
-- the thing they consented with is their address. It also means the evidence
-- outlives the record it came from — a deleted deal does not delete the proof
-- that its contact agreed, and a re-imported address does not acquire a fresh
-- one.
--
-- The address is stored ALREADY NORMALISED (lower(btrim(...)), the same fold
-- `campaign_audience` applies to its three sources) and the CHECK below holds
-- it to that. A consent row that does not join is not a near miss — it is a
-- person the tenant believes it may mail and cannot reach, or worse, one it
-- mails twice.
--
-- What is NOT here: withdrawal. An unsubscribe, a hard bounce and a complaint
-- do not delete or contradict a consent record — they suppress absolutely and
-- tenant-wide (ADR 0044 §2, queue item C1.3), which is a different table and a
-- stronger rule. Deleting the consent row instead would lose the history of
-- what the person agreed to before they changed their mind, and would let a
-- re-import quietly recreate it.
CREATE TABLE campaign_consent (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    -- The person, normalised exactly as the audience normalises its sources.
    address     TEXT NOT NULL,
    -- Where the agreement came from. 'import' and 'manual' are origins the
    -- audience has no notion of, which is why this is not the audience's
    -- source enum: an imported list is the dangerous path (ADR 0044 §2) and
    -- has to be nameable as itself rather than dressed up as a form.
    source      TEXT NOT NULL,
    -- Which form, which import, which conversation — the identifier of the
    -- thing named by `source`. Required for the origins where "which one" is
    -- the whole question; NULL where there is honestly nothing to point at.
    source_ref  TEXT,
    -- What the tenant says the person agreed to, in the tenant's own words.
    -- Mandatory: a consent record with no statement is a boolean with extra
    -- columns, and an import that cannot say where its addresses came from is
    -- precisely the case ADR 0044 §2 refuses to wave through.
    statement   TEXT NOT NULL,
    -- The colleague whose workspace recorded it. Not the person consenting —
    -- that is `address` — and not proof of anything by itself; it is who to
    -- ask when the statement turns out to be wrong.
    recorded_by TEXT NOT NULL,
    -- When the person agreed. Distinct from `recorded_at`, because an import
    -- carries consent obtained months before anybody typed it in, and dating
    -- it from the typing would overstate how fresh it is.
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT campaign_consent_source CHECK (
        source IN ('site_form', 'billing_customer', 'crm_deal', 'import', 'manual')
    ),
    CONSTRAINT campaign_consent_address_normalised CHECK (
        address = lower(btrim(address)) AND address <> '' AND octet_length(address) <= 320
    ),
    CONSTRAINT campaign_consent_statement CHECK (btrim(statement) <> '')
);

-- The one question the audience asks of this table, on every read: for each
-- address of this tenant, the most recent agreement. `occurred_at DESC, id
-- DESC` is the order the `DISTINCT ON` in `campaign_audience` walks, so the
-- newest row is the one it stops on and two rows sharing a timestamp still
-- resolve to the same one on every read.
CREATE INDEX campaign_consent_by_address
    ON campaign_consent (tenant_id, address, occurred_at DESC, id DESC);
