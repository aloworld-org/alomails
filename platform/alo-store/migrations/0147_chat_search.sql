-- alo Chat (ADR 0038): finding something that was said.
--
-- A GIN index over the message text, so searching a room's history is an index
-- lookup rather than a scan of every word a team has ever written. The
-- expression here must match the one the query uses exactly, or Postgres will
-- quietly ignore the index and the search will still work — slowly, and only
-- noticeably so once a tenant has real history.
--
-- `simple` rather than a language configuration, deliberately: chat is
-- multilingual by nature and a European workspace will hold several languages
-- in one room. Stemming for the wrong language is worse than not stemming —
-- it silently fails to match words a reader can see on the screen. The cost is
-- that "meeting" does not find "meetings"; the alternative is that German
-- text stemmed as English finds neither reliably.

CREATE INDEX chat_messages_search
    ON chat_messages USING GIN (to_tsvector('simple', body));
