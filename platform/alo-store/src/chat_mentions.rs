//! Naming someone in a message (alo Chat, ADR 0038).
//!
//! A mention is resolved **when the words are written**, not when they are
//! read. "Is there something here for me?" is asked on every sidebar draw, and
//! answering it by scanning message bodies would put a text search on the hot
//! path of every screen. Resolving once, at post time, makes it an index
//! lookup.
//!
//! Only people already in the room can be named. An `@` that matches nobody
//! there stays ordinary text — a mention that reached someone who cannot open
//! the room would be a notification pointing at a door they have no key to.

use std::collections::HashMap;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{ChatChannelId, ChatMessageId, UserId};

/// The longest handle worth considering. Comfortably past any real address,
/// short enough that a wall of text cannot become a wall of candidate handles.
const HANDLE_MAX: usize = 128;

/// Whether a character can appear in the handle after `@`. The local-part
/// characters of an address, plus `@` and `.` so a full address matches too.
fn is_handle_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-' | '@')
}

/// Every `@handle` in a message, lowercased and deduplicated, in the order
/// they appear.
///
/// A handle starts at an `@` that begins the text or follows whitespace or an
/// opening bracket — so an email address written inline (`ask disan@alo.test`)
/// does not read as a mention of `alo.test`, and neither does a price or a
/// tag. Trailing punctuation is dropped, because people write "@ben, can you"
/// and mean `ben`.
///
/// This resolves nothing: it only reports what was typed. Whether any of it
/// names a person is [`AccountStore::record_mentions`]' question.
#[must_use]
pub fn parse_handles(body: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '@' {
            i += 1;
            continue;
        }
        // An `@` only opens a handle at a boundary; inside a word it is the
        // separator of an address someone is quoting, not a mention.
        let opens = match i.checked_sub(1).map(|p| chars[p]) {
            None => true,
            Some(before) => {
                before.is_whitespace() || matches!(before, '(' | '[' | '{' | '"' | '\'')
            }
        };
        if !opens {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut end = start;
        while end < chars.len() && is_handle_char(chars[end]) && end - start < HANDLE_MAX {
            end += 1;
        }
        // "@ben," and "@ben." mean ben; a trailing dot is punctuation, never
        // part of the name.
        let mut handle: String = chars[start..end].iter().collect();
        while handle.ends_with(['.', '-', '_', '+', '%', '@']) {
            handle.pop();
        }
        if !handle.is_empty() {
            let handle = handle.to_lowercase();
            if !found.contains(&handle) {
                found.push(handle);
            }
        }
        i = end.max(start);
    }
    found
}

impl AccountStore {
    /// Resolve the handles in `body` against the room's members and record
    /// them for `message`.
    ///
    /// Replaces whatever was recorded before, so editing a message to add or
    /// remove a name does the right thing rather than leaving a stale mention
    /// behind. The author is never recorded as mentioning themselves — their
    /// own words are never news to them.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn record_mentions(
        &self,
        channel: &ChatChannelId,
        message: &ChatMessageId,
        seq: i64,
        body: &str,
    ) -> Result<Vec<UserId>> {
        sqlx::query("DELETE FROM chat_mentions WHERE tenant_id = $1 AND message_id = $2")
            .bind(self.tenant.as_str())
            .bind(message.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;

        let handles = parse_handles(body);
        if handles.is_empty() {
            return Ok(Vec::new());
        }

        // Candidates are the room's members and nobody else, so an unresolved
        // handle cannot be used to probe who exists in the tenant.
        let members = self.channel_members(channel).await?;
        let ids: Vec<UserId> = members.iter().map(|m| m.user.clone()).collect();
        let emails = crate::identity::emails_of_ids(&self.pool, self.tenant.as_str(), &ids)
            .await
            .unwrap_or_default();

        // Both spellings resolve: the full address, and the local part people
        // actually type.
        let mut by_handle: HashMap<String, UserId> = HashMap::new();
        for (user, email) in &emails {
            let email = email.to_lowercase();
            if let Some(local) = email.split('@').next() {
                by_handle
                    .entry(local.to_owned())
                    .or_insert_with(|| UserId::new(user.clone()));
            }
            by_handle
                .entry(email)
                .or_insert_with(|| UserId::new(user.clone()));
        }

        let mut named: Vec<UserId> = Vec::new();
        for handle in handles {
            let Some(user) = by_handle.get(&handle) else {
                continue;
            };
            if user.as_str() == self.user.as_str() {
                continue;
            }
            if named.iter().any(|u| u.as_str() == user.as_str()) {
                continue;
            }
            named.push(user.clone());
        }

        for user in &named {
            sqlx::query(
                "INSERT INTO chat_mentions \
                     (tenant_id, channel_id, message_id, seq, user_id) \
                 VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
            )
            .bind(self.tenant.as_str())
            .bind(channel.as_str())
            .bind(message.as_str())
            .bind(seq)
            .bind(user.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        }
        Ok(named)
    }

    /// Who is named in each of these messages, keyed by message id.
    ///
    /// One query for the page, so a feed costs one lookup rather than one per
    /// line. Messages naming nobody are simply absent.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn mentions_for_channel(
        &self,
        channel: &ChatChannelId,
        messages: &[ChatMessageId],
    ) -> Result<HashMap<String, Vec<UserId>>> {
        if messages.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<String> = messages.iter().map(|m| m.as_str().to_owned()).collect();
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT message_id, user_id FROM chat_mentions \
             WHERE tenant_id = $1 AND channel_id = $2 AND message_id = ANY($3)",
        )
        .bind(self.tenant.as_str())
        .bind(channel.as_str())
        .bind(&ids[..])
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let mut out: HashMap<String, Vec<UserId>> = HashMap::new();
        for (message_id, user_id) in rows {
            out.entry(message_id)
                .or_default()
                .push(UserId::new(user_id));
        }
        Ok(out)
    }

    /// How many unread messages name the caller, per room.
    ///
    /// Unread by the same rule everything else uses — past the reader's cursor
    /// — and tombstones do not count, because a withdrawn message has nothing
    /// left to read.
    ///
    /// # Errors
    /// [`StoreError::Db`] on a database failure.
    pub async fn unread_mentions(&self) -> Result<HashMap<String, i64>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT n.channel_id, count(*) \
             FROM chat_mentions n \
             JOIN chat_members m \
               ON m.tenant_id = n.tenant_id \
              AND m.channel_id = n.channel_id \
              AND m.user_id = n.user_id \
             JOIN chat_messages x \
               ON x.tenant_id = n.tenant_id AND x.id = n.message_id \
             WHERE n.tenant_id = $1 AND n.user_id = $2 \
               AND n.seq > m.last_read_seq \
               AND x.deleted_at IS NULL \
             GROUP BY n.channel_id",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_handles;

    #[test]
    fn a_handle_is_only_a_handle_at_a_word_boundary() {
        assert_eq!(parse_handles("@ben are you there"), vec!["ben"]);
        assert_eq!(
            parse_handles("ask ben@alo.test about it"),
            Vec::<String>::new()
        );
        assert_eq!(parse_handles("(@ben) and [@anna]"), vec!["ben", "anna"]);
    }

    #[test]
    fn trailing_punctuation_is_not_part_of_a_name() {
        assert_eq!(parse_handles("@ben, can you look"), vec!["ben"]);
        assert_eq!(parse_handles("thanks @ben."), vec!["ben"]);
        assert_eq!(parse_handles("@ben-"), vec!["ben"]);
    }

    #[test]
    fn both_spellings_are_reported_and_repeats_are_not() {
        assert_eq!(
            parse_handles("@ben and @ben again, plus @anna@alo.test"),
            vec!["ben", "anna@alo.test"]
        );
        assert_eq!(parse_handles("@BEN"), vec!["ben"], "case does not matter");
    }

    #[test]
    fn an_at_with_nothing_after_it_names_nobody() {
        assert_eq!(parse_handles("@"), Vec::<String>::new());
        assert_eq!(parse_handles("@ ben"), Vec::<String>::new());
        assert_eq!(parse_handles("email me @ the office"), Vec::<String>::new());
    }
}
