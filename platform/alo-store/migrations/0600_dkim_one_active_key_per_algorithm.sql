-- Dual signing: a domain may hold one active DKIM key **per algorithm**
-- (roadmap C2.1a).
--
-- ADR 0014 gave every domain one active key, enforced by a unique index on
-- (domain) WHERE active. That was right while every stored key was Ed25519.
-- It is wrong for bulk mail: RFC 8463 is young enough that a meaningful share
-- of receivers cannot verify an Ed25519 signature, and a campaign is exactly
-- where an unverifiable signature costs delivery. Large senders publish both
-- and let each receiver take the one it understands.
--
-- So the constraint moves from "one active key per domain" to "one active key
-- per domain per algorithm". Rotation is unchanged in shape — installing a new
-- key of an algorithm retires the previous key *of that algorithm* and leaves
-- the other alone, which is what makes the two independently rotatable.
--
-- Migrations 06xx are the mail/platform block. 05xx belongs to the campaigns
-- track, which is building alongside this.

DROP INDEX IF EXISTS dkim_keys_one_active_per_domain;

CREATE UNIQUE INDEX dkim_keys_one_active_per_domain_algorithm
    ON dkim_keys (domain, algorithm)
    WHERE active;
