-- A group may be a distribution list: give it an address and inbound mail to
-- that address fans out to every member's inbox. The address is globally unique
-- (like an alias) and stored lowercase; user and alias addresses take
-- precedence over a list address during recipient resolution.
ALTER TABLE groups ADD COLUMN address TEXT;
CREATE UNIQUE INDEX groups_address ON groups(address) WHERE address IS NOT NULL;
