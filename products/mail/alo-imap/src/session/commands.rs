//! Mailbox and message command handlers, and the view resync that turns
//! store changes into untagged EXPUNGE/EXISTS/FETCH responses. Every data
//! path runs through the passed-in [`AccountStore`], so isolation is
//! inherited: a foreign name/UID resolves to nothing and yields `NO`/empty.

use alo_store::{AccountStore, MailboxId, StoreError};
use time::OffsetDateTime;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

use super::render::{self, FetchItem};
use super::{Selected, Session, State, ViewEntry};
use crate::parser::{AString, Parser, SequenceSet};
use crate::{fetch, flags, mailbox, search};

impl Session {
    // ---- view sync -----------------------------------------------------

    /// Re-reads the selected mailbox and emits pending untagged responses:
    /// `* n EXPUNGE` (ascending, each number decrement-adjusted for the
    /// removals already reported), `* n EXISTS` for arrivals, and
    /// `* n FETCH (FLAGS ...)` for flag changes. Updates the snapshot.
    pub(super) async fn resync(&mut self, acc: &AccountStore) -> std::io::Result<()> {
        let Some(sel) = self.selected.as_ref() else {
            return Ok(());
        };
        let id = sel.id.clone();
        let old = sel.view.clone();
        let new_entries = match acc.imap_view(&id).await {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        let new_view: Vec<ViewEntry> = new_entries
            .into_iter()
            .map(|e| ViewEntry {
                uid: e.uid,
                message: e.message,
                flags: e.flags,
            })
            .collect();
        let new_uids: std::collections::HashSet<i64> = new_view.iter().map(|e| e.uid).collect();

        // EXPUNGE pass: walk the OLD view keeping a live sequence number.
        // A removed entry keeps its number for the response; entries after
        // it shift down (so we do NOT advance on a removal).
        let mut seq = 1usize;
        let mut expunges = Vec::new();
        for e in &old {
            if new_uids.contains(&e.uid) {
                seq += 1;
            } else {
                expunges.push(format!("* {seq} EXPUNGE"));
            }
        }
        for line in &expunges {
            self.send_line(line).await?;
        }

        // Arrivals → EXISTS with the new count.
        let old_uids: std::collections::HashSet<i64> = old.iter().map(|e| e.uid).collect();
        let arrivals = new_view.iter().any(|e| !old_uids.contains(&e.uid));
        if arrivals || !expunges.is_empty() {
            // EXISTS always reports the current count after any change.
            self.send_line(&format!("* {} EXISTS", new_view.len()))
                .await?;
        }

        // Flag changes for surviving messages.
        let mut fetches = Vec::new();
        for (i, e) in new_view.iter().enumerate() {
            if let Some(prev) = old.iter().find(|o| o.uid == e.uid)
                && !same_flags(&prev.flags, &e.flags)
            {
                fetches.push(format!(
                    "* {} FETCH (UID {} FLAGS ({}))",
                    i + 1,
                    e.uid,
                    flags::render_flags(&e.flags)
                ));
            }
        }
        for line in &fetches {
            self.send_line(line).await?;
        }

        let synced_state = acc.state().await.unwrap_or_default();
        if let Some(sel) = self.selected.as_mut() {
            sel.view = new_view;
            sel.synced_state = synced_state;
        }
        Ok(())
    }

    /// The selected view and read-only flag, or `None` (caller sends NO).
    fn view_snapshot(&self) -> Option<(MailboxId, Vec<ViewEntry>, bool)> {
        self.selected
            .as_ref()
            .map(|s| (s.id.clone(), s.view.clone(), s.read_only))
    }

    // ---- SELECT / EXAMINE ---------------------------------------------

    pub(super) async fn cmd_select(
        &mut self,
        tag: &str,
        p: &mut Parser,
        examine: bool,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some(name) = p.read_astring().map(|a| a.as_string()) else {
            return self.tagged(tag, "BAD", "SELECT requires a mailbox").await;
        };
        // Leaving Selected always returns to Authenticated first (RFC 9051).
        self.selected = None;
        self.state = State::Auth;

        let mailboxes = match acc.imap_mailboxes().await {
            Ok(m) => m,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        let Some(mb) = mailbox::resolve(&mailboxes, &name).cloned() else {
            return self.tagged(tag, "NO", "Mailbox does not exist").await;
        };
        let entries = match acc.imap_view(&mb.id).await {
            Ok(v) => v,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        let view: Vec<ViewEntry> = entries
            .into_iter()
            .map(|e| ViewEntry {
                uid: e.uid,
                message: e.message,
                flags: e.flags,
            })
            .collect();
        let first_unseen = view
            .iter()
            .position(|e| !e.flags.iter().any(|f| f == "$seen"));
        let synced_state = acc.state().await.unwrap_or_default();

        self.send_line(&format!("* {} EXISTS", view.len())).await?;
        self.send_line("* 0 RECENT").await?;
        self.send_line("* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)")
            .await?;
        if examine {
            self.send_line("* OK [PERMANENTFLAGS ()] Read-only mailbox")
                .await?;
        } else {
            self.send_line(
                "* OK [PERMANENTFLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft \\*)] Limited",
            )
            .await?;
        }
        self.send_line(&format!(
            "* OK [UIDVALIDITY {}] UIDs valid",
            mb.uid_validity
        ))
        .await?;
        self.send_line(&format!(
            "* OK [UIDNEXT {}] Predicted next UID",
            mb.uid_next
        ))
        .await?;
        if let Some(pos) = first_unseen {
            self.send_line(&format!("* OK [UNSEEN {}] First unseen", pos + 1))
                .await?;
        }

        self.selected = Some(Selected {
            id: mb.id,
            read_only: examine,
            view,
            synced_state,
        });
        self.state = State::Selected;
        let (verb, access) = if examine {
            ("EXAMINE", "[READ-ONLY]")
        } else {
            ("SELECT", "[READ-WRITE]")
        };
        self.tagged(tag, "OK", &format!("{access} {verb} completed"))
            .await
    }

    // ---- CREATE / DELETE / RENAME -------------------------------------

    pub(super) async fn cmd_create(
        &mut self,
        tag: &str,
        p: &mut Parser,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some(mut name) = p.read_astring().map(|a| a.as_string()) else {
            return self.tagged(tag, "BAD", "CREATE requires a name").await;
        };
        // A trailing separator means "directory"; drop it (RFC 9051 §6.3.4).
        while name.ends_with(mailbox::SEP) {
            name.pop();
        }
        if name.is_empty() || !valid_mailbox_name(&name) {
            return self.tagged(tag, "NO", "Invalid mailbox name").await;
        }
        let mailboxes = match acc.imap_mailboxes().await {
            Ok(m) => m,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        if mailbox::resolve(&mailboxes, &name).is_some() {
            return self
                .tagged(tag, "NO", "[ALREADYEXISTS] Mailbox exists")
                .await;
        }
        let (parent_path, leaf) = match name.rsplit_once(mailbox::SEP) {
            Some((pp, leaf)) => (Some(pp), leaf),
            None => (None, name.as_str()),
        };
        let parent = match parent_path {
            Some(pp) => match mailbox::resolve(&mailboxes, pp) {
                Some(m) => Some(m.id.clone()),
                None => {
                    return self
                        .tagged(tag, "NO", "Parent mailbox does not exist")
                        .await;
                }
            },
            None => None,
        };
        match acc.create_mailbox(parent.as_ref(), leaf, None).await {
            Ok(_) => self.tagged(tag, "OK", "CREATE completed").await,
            Err(StoreError::Conflict(_)) => {
                self.tagged(tag, "NO", "[ALREADYEXISTS] Mailbox exists")
                    .await
            }
            Err(_) => self.tagged(tag, "NO", "CREATE failed").await,
        }
    }

    pub(super) async fn cmd_delete(
        &mut self,
        tag: &str,
        p: &mut Parser,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some(name) = p.read_astring().map(|a| a.as_string()) else {
            return self.tagged(tag, "BAD", "DELETE requires a name").await;
        };
        if name.eq_ignore_ascii_case(mailbox::INBOX) {
            return self.tagged(tag, "NO", "Cannot delete INBOX").await;
        }
        let mailboxes = match acc.imap_mailboxes().await {
            Ok(m) => m,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        let Some(mb) = mailbox::resolve(&mailboxes, &name).cloned() else {
            return self.tagged(tag, "NO", "Mailbox does not exist").await;
        };
        if mailbox::has_children(&mailboxes, &mb) {
            return self
                .tagged(tag, "NO", "Mailbox has inferior hierarchical names")
                .await;
        }
        // Empty the mailbox (IMAP DELETE removes its messages), then drop it.
        if let Ok(view) = acc.imap_view(&mb.id).await {
            for e in view {
                let _ = acc.imap_expunge(&mb.id, &e.message).await;
            }
        }
        match acc.destroy_mailbox(&mb.id).await {
            Ok(()) => self.tagged(tag, "OK", "DELETE completed").await,
            Err(_) => self.tagged(tag, "NO", "DELETE failed").await,
        }
    }

    pub(super) async fn cmd_rename(
        &mut self,
        tag: &str,
        p: &mut Parser,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let old = p.read_astring().map(|a| a.as_string());
        p.skip_sp();
        let new = p.read_astring().map(|a| a.as_string());
        let (Some(old), Some(mut new)) = (old, new) else {
            return self.tagged(tag, "BAD", "RENAME requires two names").await;
        };
        while new.ends_with(mailbox::SEP) {
            new.pop();
        }
        if new.is_empty() || !valid_mailbox_name(&new) {
            return self.tagged(tag, "NO", "Invalid mailbox name").await;
        }
        if old.eq_ignore_ascii_case(mailbox::INBOX) {
            return self
                .tagged(tag, "NO", "Renaming INBOX is not supported")
                .await;
        }
        let mailboxes = match acc.imap_mailboxes().await {
            Ok(m) => m,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        let Some(mb) = mailbox::resolve(&mailboxes, &old).cloned() else {
            return self.tagged(tag, "NO", "Mailbox does not exist").await;
        };
        if mailbox::resolve(&mailboxes, &new).is_some() {
            return self
                .tagged(tag, "NO", "[ALREADYEXISTS] Target exists")
                .await;
        }
        let (parent_path, leaf) = match new.rsplit_once(mailbox::SEP) {
            Some((pp, leaf)) => (Some(pp), leaf.to_owned()),
            None => (None, new.clone()),
        };
        let new_parent = match parent_path {
            Some(pp) => match mailbox::resolve(&mailboxes, pp) {
                Some(m) => Some(m.id.clone()),
                None => return self.tagged(tag, "NO", "Target parent does not exist").await,
            },
            None => None,
        };
        if new_parent.as_ref() != mb.parent_id.as_ref()
            && let Err(e) = acc.move_mailbox(&mb.id, new_parent.as_ref()).await
        {
            return self.rename_err(tag, e).await;
        }
        match acc.rename_mailbox(&mb.id, &leaf).await {
            Ok(()) => self.tagged(tag, "OK", "RENAME completed").await,
            Err(e) => self.rename_err(tag, e).await,
        }
    }

    async fn rename_err(&mut self, tag: &str, e: StoreError) -> std::io::Result<()> {
        match e {
            StoreError::Conflict(_) => {
                self.tagged(tag, "NO", "[ALREADYEXISTS] Rename conflict")
                    .await
            }
            StoreError::NotFound => self.tagged(tag, "NO", "Mailbox does not exist").await,
            _ => self.tagged(tag, "NO", "RENAME failed").await,
        }
    }

    // ---- LIST / LSUB ---------------------------------------------------

    pub(super) async fn cmd_list(
        &mut self,
        tag: &str,
        p: &mut Parser,
        lsub: bool,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let reference = p.read_astring().map(|a| a.as_string()).unwrap_or_default();
        p.skip_sp();
        let pattern = p.read_astring().map(|a| a.as_string()).unwrap_or_default();
        let verb = if lsub { "LSUB" } else { "LIST" };

        // A empty pattern is a request for the hierarchy delimiter.
        if pattern.is_empty() {
            self.send_line(&format!("* {verb} (\\Noselect) \"/\" \"\""))
                .await?;
            return self.tagged(tag, "OK", &format!("{verb} completed")).await;
        }
        let full = format!("{reference}{pattern}");
        let mailboxes = match acc.imap_mailboxes().await {
            Ok(m) => m,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        for mb in &mailboxes {
            let path = mailbox::path_of(&mailboxes, mb);
            if !mailbox::list_match(&full, &path) {
                continue;
            }
            let mut attrs = Vec::new();
            let su = mailbox::special_use(mb.role.as_deref());
            if !su.is_empty() {
                attrs.push(su.to_owned());
            }
            if mailbox::has_children(&mailboxes, mb) {
                attrs.push("\\HasChildren".to_owned());
            } else {
                attrs.push("\\HasNoChildren".to_owned());
            }
            self.send_line(&format!(
                "* {verb} ({}) \"/\" {}",
                attrs.join(" "),
                fetch::quote(&path)
            ))
            .await?;
        }
        self.tagged(tag, "OK", &format!("{verb} completed")).await
    }

    // ---- STATUS --------------------------------------------------------

    pub(super) async fn cmd_status(
        &mut self,
        tag: &str,
        p: &mut Parser,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some(name) = p.read_astring().map(|a| a.as_string()) else {
            return self.tagged(tag, "BAD", "STATUS requires a mailbox").await;
        };
        p.skip_sp();
        let items = p.rest();
        let mailboxes = match acc.imap_mailboxes().await {
            Ok(m) => m,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        let Some(mb) = mailbox::resolve(&mailboxes, &name).cloned() else {
            return self.tagged(tag, "NO", "Mailbox does not exist").await;
        };
        let inner = items.trim().trim_start_matches('(').trim_end_matches(')');
        let mut out = Vec::new();
        for item in inner.split_whitespace() {
            let value = match item.to_ascii_uppercase().as_str() {
                "MESSAGES" => mb.total_messages,
                "RECENT" => 0,
                "UIDNEXT" => mb.uid_next,
                "UIDVALIDITY" => mb.uid_validity,
                "UNSEEN" => mb.unread_messages,
                "DELETED" => 0,
                "SIZE" => 0,
                _ => continue,
            };
            out.push(format!("{} {}", item.to_ascii_uppercase(), value));
        }
        let path = mailbox::path_of(&mailboxes, &mb);
        self.send_line(&format!(
            "* STATUS {} ({})",
            fetch::quote(&path),
            out.join(" ")
        ))
        .await?;
        self.tagged(tag, "OK", "STATUS completed").await
    }

    // ---- APPEND --------------------------------------------------------

    pub(super) async fn cmd_append(
        &mut self,
        tag: &str,
        p: &mut Parser,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some(name) = p.read_astring().map(|a| a.as_string()) else {
            return self.tagged(tag, "BAD", "APPEND requires a mailbox").await;
        };
        p.skip_sp();
        let mut kw = Vec::new();
        if p.peek() == Some('(') {
            for f in read_flag_list(p) {
                if let Some(k) = flags::imap_to_keyword(&f) {
                    kw.push(k);
                }
            }
            p.skip_sp();
        }
        let mut internaldate = None;
        if p.peek() == Some('"') {
            if let Some(dt) = p.read_quoted() {
                internaldate = parse_append_date(&dt);
            }
            p.skip_sp();
        }
        let Some(msg) = p.read_astring() else {
            return self
                .tagged(tag, "BAD", "APPEND requires a message literal")
                .await;
        };
        let raw = match msg {
            AString::Bytes(b) => b,
            AString::Str(s) => s.into_bytes(),
        };
        let mailboxes = match acc.imap_mailboxes().await {
            Ok(m) => m,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        let Some(mb) = mailbox::resolve(&mailboxes, &name).cloned() else {
            return self
                .tagged(tag, "NO", "[TRYCREATE] Mailbox does not exist")
                .await;
        };
        match acc.imap_append(&mb.id, &raw, internaldate).await {
            Ok((message, uid)) => {
                for k in &kw {
                    let _ = acc.set_keyword(&message, k, true).await;
                }
                // If the target is the selected mailbox, show the arrival.
                self.resync(acc).await?;
                self.tagged(
                    tag,
                    "OK",
                    &format!("[APPENDUID {} {}] APPEND completed", mb.uid_validity, uid),
                )
                .await
            }
            Err(StoreError::TooLarge { .. }) => {
                self.tagged(tag, "NO", "[LIMIT] Message too large").await
            }
            Err(_) => self.tagged(tag, "NO", "APPEND failed").await,
        }
    }

    // ---- CLOSE / UNSELECT / EXPUNGE -----------------------------------

    pub(super) async fn cmd_close(
        &mut self,
        tag: &str,
        expunge: bool,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some((id, _, read_only)) = self.view_snapshot() else {
            return self.tagged(tag, "NO", "No mailbox selected").await;
        };
        if expunge
            && !read_only
            && let Ok(deleted) = acc.imap_flagged_uids(&id, flags::DELETED).await
        {
            for (_uid, message) in deleted {
                let _ = acc.imap_expunge(&id, &message).await;
            }
        }
        self.selected = None;
        self.state = State::Auth;
        let verb = if expunge { "CLOSE" } else { "UNSELECT" };
        self.tagged(tag, "OK", &format!("{verb} completed")).await
    }

    pub(super) async fn cmd_expunge(
        &mut self,
        tag: &str,
        uid_set: Option<SequenceSet>,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some((id, _, read_only)) = self.view_snapshot() else {
            return self.tagged(tag, "NO", "No mailbox selected").await;
        };
        if read_only {
            return self.tagged(tag, "NO", "Mailbox is read-only").await;
        }
        let deleted = acc
            .imap_flagged_uids(&id, flags::DELETED)
            .await
            .unwrap_or_default();
        let targets: Vec<_> = match &uid_set {
            Some(set) => {
                let uids: Vec<i64> = deleted.iter().map(|(u, _)| *u).collect();
                let chosen = set.resolve_uids(&uids);
                deleted
                    .into_iter()
                    .filter(|(u, _)| chosen.contains(u))
                    .collect()
            }
            None => deleted,
        };
        for (_uid, message) in targets {
            let _ = acc.imap_expunge(&id, &message).await;
        }
        // resync emits the * n EXPUNGE responses and updates the view.
        self.resync(acc).await?;
        self.tagged(tag, "OK", "EXPUNGE completed").await
    }

    // ---- FETCH ---------------------------------------------------------

    pub(super) async fn cmd_fetch(
        &mut self,
        tag: &str,
        p: &mut Parser,
        uid: bool,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some((_id, view, read_only)) = self.view_snapshot() else {
            return self.tagged(tag, "NO", "No mailbox selected").await;
        };
        let set_str = p.read_atom();
        p.skip_sp();
        let items_str = p.rest();
        let Some(set) = SequenceSet::parse(&set_str) else {
            return self.tagged(tag, "BAD", "Invalid sequence set").await;
        };
        let Some(items) = render::parse_items(&items_str) else {
            return self.tagged(tag, "BAD", "Invalid FETCH items").await;
        };
        let needs = render::needs(&items);
        let targets = resolve_targets(&view, &set, uid);
        let mut seen_set: Vec<i64> = Vec::new();

        for (seq, entry) in targets {
            let meta = match acc.message(&entry.message).await {
                Ok(m) => m,
                Err(_) => continue, // vanished concurrently — skip
            };
            let raw = if needs.bytes {
                acc.message_bytes(&entry.message).await.ok()
            } else {
                None
            };
            let mut eff_items = items.clone();
            let mut flags_now = entry.flags.clone();
            if needs.mark_seen && !read_only && !flags_now.iter().any(|f| f == "$seen") {
                let _ = acc.set_keyword(&entry.message, "$seen", true).await;
                flags_now.push("$seen".to_owned());
                seen_set.push(entry.uid);
                if !eff_items.iter().any(|i| matches!(i, FetchItem::Flags)) {
                    eff_items.insert(0, FetchItem::Flags);
                }
            }
            let line = render::render_fetch(
                seq,
                entry.uid,
                &flags_now,
                meta.received_at,
                meta.size,
                raw.as_deref(),
                &eff_items,
                uid,
                &entry.message,
            );
            self.send(&line).await?;
        }
        // Reflect any \Seen we set into the snapshot without re-emitting.
        if !seen_set.is_empty()
            && let Some(sel) = self.selected.as_mut()
        {
            for e in &mut sel.view {
                if seen_set.contains(&e.uid) && !e.flags.iter().any(|f| f == "$seen") {
                    e.flags.push("$seen".to_owned());
                }
            }
            sel.synced_state = acc.state().await.unwrap_or_default();
        }
        self.tagged(tag, "OK", "FETCH completed").await
    }

    // ---- STORE ---------------------------------------------------------

    pub(super) async fn cmd_store(
        &mut self,
        tag: &str,
        p: &mut Parser,
        uid: bool,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some((_id, view, read_only)) = self.view_snapshot() else {
            return self.tagged(tag, "NO", "No mailbox selected").await;
        };
        if read_only {
            return self.tagged(tag, "NO", "Mailbox is read-only").await;
        }
        let set_str = p.read_atom();
        p.skip_sp();
        let op = p.read_atom().to_ascii_uppercase();
        p.skip_sp();
        let new_flags: Vec<String> = read_flag_list(p)
            .iter()
            .filter_map(|f| flags::imap_to_keyword(f))
            .collect();
        let Some(set) = SequenceSet::parse(&set_str) else {
            return self.tagged(tag, "BAD", "Invalid sequence set").await;
        };
        let (mode, silent) = match op.as_str() {
            "FLAGS" => (StoreMode::Replace, false),
            "FLAGS.SILENT" => (StoreMode::Replace, true),
            "+FLAGS" => (StoreMode::Add, false),
            "+FLAGS.SILENT" => (StoreMode::Add, true),
            "-FLAGS" => (StoreMode::Remove, false),
            "-FLAGS.SILENT" => (StoreMode::Remove, true),
            _ => return self.tagged(tag, "BAD", "Invalid STORE operation").await,
        };
        let targets = resolve_targets(&view, &set, uid);
        for (seq, entry) in targets {
            let final_flags = apply_store(acc, &entry, &new_flags, mode).await;
            if let Some(sel) = self.selected.as_mut()
                && let Some(v) = sel.view.iter_mut().find(|v| v.uid == entry.uid)
            {
                v.flags = final_flags.clone();
            }
            if !silent {
                let mut line = format!(
                    "* {seq} FETCH (FLAGS ({})",
                    flags::render_flags(&final_flags)
                );
                if uid {
                    line.push_str(&format!(" UID {}", entry.uid));
                }
                line.push(')');
                self.send_line(&line).await?;
            }
        }
        if let Some(sel) = self.selected.as_mut() {
            sel.synced_state = acc.state().await.unwrap_or_default();
        }
        self.tagged(tag, "OK", "STORE completed").await
    }

    // ---- COPY / MOVE ---------------------------------------------------

    pub(super) async fn cmd_copy(
        &mut self,
        tag: &str,
        p: &mut Parser,
        uid: bool,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some((_id, view, _)) = self.view_snapshot() else {
            return self.tagged(tag, "NO", "No mailbox selected").await;
        };
        let set_str = p.read_atom();
        p.skip_sp();
        let Some(dest_name) = p.read_astring().map(|a| a.as_string()) else {
            return self.tagged(tag, "BAD", "COPY requires a destination").await;
        };
        let Some(set) = SequenceSet::parse(&set_str) else {
            return self.tagged(tag, "BAD", "Invalid sequence set").await;
        };
        let mailboxes = match acc.imap_mailboxes().await {
            Ok(m) => m,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        let Some(dest) = mailbox::resolve(&mailboxes, &dest_name).cloned() else {
            return self
                .tagged(tag, "NO", "[TRYCREATE] Destination does not exist")
                .await;
        };
        let targets = resolve_targets(&view, &set, uid);
        let mut src_uids = Vec::new();
        let mut dst_uids = Vec::new();
        for (_seq, entry) in targets {
            if acc.add_to_mailbox(&entry.message, &dest.id).await.is_ok()
                && let Ok(Some(new_uid)) = acc.imap_uid_of(&dest.id, &entry.message).await
            {
                src_uids.push(entry.uid);
                dst_uids.push(new_uid);
            }
        }
        self.resync(acc).await?;
        let code = copyuid_code(dest.uid_validity, &src_uids, &dst_uids);
        self.tagged(tag, "OK", &format!("{code}COPY completed"))
            .await
    }

    pub(super) async fn cmd_move(
        &mut self,
        tag: &str,
        p: &mut Parser,
        uid: bool,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some((id, view, read_only)) = self.view_snapshot() else {
            return self.tagged(tag, "NO", "No mailbox selected").await;
        };
        if read_only {
            return self.tagged(tag, "NO", "Mailbox is read-only").await;
        }
        let set_str = p.read_atom();
        p.skip_sp();
        let Some(dest_name) = p.read_astring().map(|a| a.as_string()) else {
            return self.tagged(tag, "BAD", "MOVE requires a destination").await;
        };
        let Some(set) = SequenceSet::parse(&set_str) else {
            return self.tagged(tag, "BAD", "Invalid sequence set").await;
        };
        let mailboxes = match acc.imap_mailboxes().await {
            Ok(m) => m,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        let Some(dest) = mailbox::resolve(&mailboxes, &dest_name).cloned() else {
            return self
                .tagged(tag, "NO", "[TRYCREATE] Destination does not exist")
                .await;
        };
        // MOVE into the source mailbox is a no-op: the messages are already
        // there, so we must NOT expunge them (that would destroy them).
        if dest.id == id {
            return self.tagged(tag, "OK", "MOVE completed").await;
        }
        let targets = resolve_targets(&view, &set, uid);
        let mut src_uids = Vec::new();
        let mut dst_uids = Vec::new();
        for (_seq, entry) in targets {
            if acc.add_to_mailbox(&entry.message, &dest.id).await.is_ok() {
                if let Ok(Some(new_uid)) = acc.imap_uid_of(&dest.id, &entry.message).await {
                    src_uids.push(entry.uid);
                    dst_uids.push(new_uid);
                }
                // Remove from the source mailbox (destroying if now orphaned).
                let _ = acc.imap_expunge(&id, &entry.message).await;
            }
        }
        let code = copyuid_code(dest.uid_validity, &src_uids, &dst_uids);
        // resync emits the EXPUNGE responses for the source.
        self.resync(acc).await?;
        self.tagged(tag, "OK", &format!("{code}MOVE completed"))
            .await
    }

    // ---- SEARCH --------------------------------------------------------

    pub(super) async fn cmd_search(
        &mut self,
        tag: &str,
        p: &mut Parser,
        uid: bool,
        acc: &AccountStore,
    ) -> std::io::Result<()> {
        let Some((id, view, _)) = self.view_snapshot() else {
            return self.tagged(tag, "NO", "No mailbox selected").await;
        };
        let rest = p.rest();
        let mut tokens = search::tokenize(&rest);
        // Optional leading CHARSET (accept US-ASCII/UTF-8 only).
        if tokens.first().map(|t| t.eq_ignore_ascii_case("CHARSET")) == Some(true) {
            let cs = tokens.get(1).cloned().unwrap_or_default();
            if !cs.eq_ignore_ascii_case("UTF-8") && !cs.eq_ignore_ascii_case("US-ASCII") {
                return self
                    .tagged(
                        tag,
                        "NO",
                        "[BADCHARSET (US-ASCII UTF-8)] Unsupported charset",
                    )
                    .await;
            }
            tokens.drain(0..2);
        }
        let Some(key) = search::parse(&tokens) else {
            return self.tagged(tag, "BAD", "Invalid search criteria").await;
        };
        let rows = match acc.imap_search_rows(&id).await {
            Ok(r) => r,
            Err(_) => return self.tagged(tag, "NO", "Server error").await,
        };
        let all_uids: Vec<i64> = view.iter().map(|e| e.uid).collect();
        let view_len = view.len();
        let seq_match = |set: &str, seq: u64| {
            SequenceSet::parse(set)
                .map(|s| {
                    s.resolve_indices(view_len)
                        .iter()
                        .any(|&i| i as u64 + 1 == seq)
                })
                .unwrap_or(false)
        };
        let uid_match = |set: &str, u: i64| {
            SequenceSet::parse(set)
                .map(|s| s.resolve_uids(&all_uids).contains(&u))
                .unwrap_or(false)
        };
        let need_bytes = key.needs_bytes();
        // Number sequence hits against the session VIEW (the client's
        // sequence space), not the freshly-read rows, so a stale row order
        // can never mislabel. Index rows by UID.
        let by_uid: std::collections::HashMap<i64, &alo_store::ImapSearchRow> =
            rows.iter().map(|r| (r.uid, r)).collect();
        let mut hits: Vec<u64> = Vec::new();
        let mut scanned: usize = 0;
        let mut truncated = false;
        for (i, entry) in view.iter().enumerate() {
            let Some(row) = by_uid.get(&entry.uid) else {
                continue; // in the view but not in the fresh read (raced away)
            };
            let seq = (i + 1) as u64;
            let bytes = if need_bytes && !truncated {
                match acc.message_bytes(&row.message).await {
                    Ok(b) => {
                        scanned = scanned.saturating_add(b.len());
                        if scanned > MAX_SEARCH_SCAN {
                            truncated = true;
                        }
                        Some(b)
                    }
                    Err(_) => None,
                }
            } else {
                None
            };
            if search::eval(&key, row, seq, bytes.as_deref(), &seq_match, &uid_match) {
                hits.push(if uid { entry.uid as u64 } else { seq });
            }
        }
        if truncated {
            // Never silently under-report: a body/text search that exceeded
            // the scan budget is logged (ids only, no content).
            tracing::warn!(
                mailbox = %id,
                budget = MAX_SEARCH_SCAN,
                "SEARCH body scan hit the budget; later messages not body-matched"
            );
        }
        let list = hits
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        if list.is_empty() {
            self.send_line("* SEARCH").await?;
        } else {
            self.send_line(&format!("* SEARCH {list}")).await?;
        }
        self.tagged(tag, "OK", "SEARCH completed").await
    }
}

/// STORE flag operation.
#[derive(Clone, Copy)]
enum StoreMode {
    Replace,
    Add,
    Remove,
}

/// Applies a STORE to one message, returning its final keyword set.
async fn apply_store(
    acc: &AccountStore,
    entry: &ViewEntry,
    new_flags: &[String],
    mode: StoreMode,
) -> Vec<String> {
    let mut current = entry.flags.clone();
    match mode {
        StoreMode::Add => {
            for f in new_flags {
                if !current.contains(f) {
                    let _ = acc.set_keyword(&entry.message, f, true).await;
                    current.push(f.clone());
                }
            }
        }
        StoreMode::Remove => {
            for f in new_flags {
                if current.contains(f) {
                    let _ = acc.set_keyword(&entry.message, f, false).await;
                    current.retain(|c| c != f);
                }
            }
        }
        StoreMode::Replace => {
            // Remove any not in the new set, add any missing.
            let to_remove: Vec<String> = current
                .iter()
                .filter(|c| !new_flags.contains(c))
                .cloned()
                .collect();
            for f in &to_remove {
                let _ = acc.set_keyword(&entry.message, f, false).await;
            }
            for f in new_flags {
                if !current.contains(f) {
                    let _ = acc.set_keyword(&entry.message, f, true).await;
                }
            }
            current = new_flags.to_vec();
        }
    }
    current
}

/// Resolves a sequence/UID set against the view to `(seq, entry)` targets.
fn resolve_targets(view: &[ViewEntry], set: &SequenceSet, uid: bool) -> Vec<(usize, ViewEntry)> {
    if uid {
        let uids: Vec<i64> = view.iter().map(|e| e.uid).collect();
        set.resolve_uids(&uids)
            .into_iter()
            .filter_map(|u| {
                view.iter()
                    .position(|e| e.uid == u)
                    .map(|i| (i + 1, view[i].clone()))
            })
            .collect()
    } else {
        set.resolve_indices(view.len())
            .into_iter()
            .map(|i| (i + 1, view[i].clone()))
            .collect()
    }
}

/// Reads a flag list — `(\Seen \Flagged)` or a single bare flag.
fn read_flag_list(p: &mut Parser) -> Vec<String> {
    p.skip_sp();
    let mut flags = Vec::new();
    if p.eat('(') {
        loop {
            p.skip_sp();
            if p.eat(')') {
                break;
            }
            let f = read_flag(p);
            if f.is_empty() {
                break;
            }
            flags.push(f);
        }
    } else {
        let f = read_flag(p);
        if !f.is_empty() {
            flags.push(f);
        }
    }
    flags
}

/// Reads one flag token (a leading `\` plus atom, or a keyword atom).
fn read_flag(p: &mut Parser) -> String {
    let backslash = p.eat('\\');
    let atom = p.read_atom();
    if backslash { format!("\\{atom}") } else { atom }
}

/// Byte budget for a single SEARCH's body/text scan — bounds the I/O a
/// `SEARCH BODY x` on a huge mailbox can pin to one worker.
const MAX_SEARCH_SCAN: usize = 256 * 1024 * 1024;

/// A mailbox name is valid if it carries no control characters — a CR/LF
/// in a name would otherwise splice extra untagged lines when the name is
/// echoed by LIST/STATUS (response injection).
fn valid_mailbox_name(name: &str) -> bool {
    !name.chars().any(|c| c.is_control())
}

fn same_flags(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut a: Vec<&String> = a.iter().collect();
    let mut b: Vec<&String> = b.iter().collect();
    a.sort();
    b.sort();
    a == b
}

/// A `[COPYUID validity src dst] ` response-code prefix, or empty when
/// nothing was copied.
fn copyuid_code(uid_validity: i64, src: &[i64], dst: &[i64]) -> String {
    if src.is_empty() {
        return String::new();
    }
    let s = src.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let d = dst.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    format!("[COPYUID {uid_validity} {s} {d}] ")
}

const APPEND_FMTS: &[&[BorrowedFormatItem<'_>]] = &[
    format_description!(
        "[day padding:zero]-[month repr:short]-[year] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]"
    ),
    format_description!(
        "[day padding:space]-[month repr:short]-[year] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]"
    ),
];

/// Parses an IMAP APPEND date-time; `None` (→ now) on failure.
fn parse_append_date(s: &str) -> Option<OffsetDateTime> {
    let s = s.trim();
    for fmt in APPEND_FMTS {
        if let Ok(dt) = OffsetDateTime::parse(s, fmt) {
            return Some(dt);
        }
    }
    None
}
