//! The evaluator: runs a compiled [`Script`] against a [`Message`] and
//! returns an [`Outcome`]. Pure — no I/O. Every command and test node
//! charges the instruction budget; overrun aborts to [`EvalError`], and the
//! caller falls back to implicit keep (mail is never lost). Implicit keep
//! (RFC 5228 §2.10.2) is resolved into the returned action list.

use crate::action::{Action, EvalError, Outcome, VacationReply};
use crate::ast::*;
use crate::message::{Address, Message};
use crate::parser::Script;

/// Per-account context the evaluator needs beyond the message: the owner's
/// own addresses (for vacation "to me"/"from me") and the per-script
/// redirect cap.
#[derive(Debug, Clone)]
pub struct EvalContext {
    /// The account's own addresses (lowercased), for vacation guards and
    /// self-redirect prevention.
    pub owner_addresses: Vec<String>,
    /// Max `redirect` actions emitted per evaluation (amplification cap).
    pub max_redirects: usize,
}

impl EvalContext {
    /// A context for one account, with the default redirect cap (3).
    pub fn new(owner_addresses: Vec<String>) -> Self {
        Self {
            owner_addresses: owner_addresses
                .into_iter()
                .map(|a| a.to_ascii_lowercase())
                .collect(),
            max_redirects: 3,
        }
    }

    fn owns(&self, addr: &str) -> bool {
        let a = addr.to_ascii_lowercase();
        self.owner_addresses.contains(&a)
    }
}

/// Evaluates `script` against `message` in `ctx`.
///
/// # Errors
/// [`EvalError::BudgetExceeded`] if the instruction budget is exhausted;
/// the caller responds with implicit keep.
pub fn evaluate(
    script: &Script,
    message: &Message,
    ctx: &EvalContext,
) -> Result<Outcome, EvalError> {
    let mut e = Eval {
        msg: message,
        ctx,
        budget: script.limits.max_instructions,
        used: 0,
        internal_flags: Vec::new(),
        actions: Vec::new(),
        warnings: Vec::new(),
        cancel_keep: false,
        stopped: false,
        redirects: 0,
    };
    e.run(&script.commands)?;
    if !e.cancel_keep {
        // Implicit keep into the Inbox with whatever flags are set.
        e.actions.push(Action::Keep {
            flags: e.internal_flags.clone(),
        });
    }
    Ok(Outcome {
        actions: e.actions,
        warnings: e.warnings,
    })
}

struct Eval<'a> {
    msg: &'a Message,
    ctx: &'a EvalContext,
    budget: u64,
    used: u64,
    internal_flags: Vec<String>,
    actions: Vec<Action>,
    warnings: Vec<String>,
    cancel_keep: bool,
    stopped: bool,
    redirects: usize,
}

impl Eval<'_> {
    fn tick(&mut self) -> Result<(), EvalError> {
        self.used += 1;
        if self.used > self.budget {
            return Err(EvalError::BudgetExceeded);
        }
        Ok(())
    }

    fn run(&mut self, cmds: &[Command]) -> Result<(), EvalError> {
        for cmd in cmds {
            if self.stopped {
                break;
            }
            self.tick()?;
            self.exec(cmd)?;
        }
        Ok(())
    }

    fn exec(&mut self, cmd: &Command) -> Result<(), EvalError> {
        match cmd {
            Command::Require(_) => {}
            Command::Stop => self.stopped = true,
            Command::Keep(flags) => {
                self.cancel_keep = true;
                let flags = flags.clone().unwrap_or_else(|| self.internal_flags.clone());
                self.actions.push(Action::Keep { flags });
            }
            Command::Discard => {
                // Explicitly cancel implicit keep; file nowhere.
                self.cancel_keep = true;
            }
            Command::FileInto { flags, mailbox } => {
                self.cancel_keep = true;
                let flags = flags.clone().unwrap_or_else(|| self.internal_flags.clone());
                self.actions.push(Action::FileInto {
                    mailbox: mailbox.clone(),
                    flags,
                });
            }
            Command::Redirect(address) => self.exec_redirect(address),
            Command::Vacation(v) => self.exec_vacation(v),
            Command::SetFlag(f) => self.internal_flags = f.clone(),
            Command::AddFlag(f) => {
                for x in f {
                    if !self.internal_flags.contains(x) {
                        self.internal_flags.push(x.clone());
                    }
                }
            }
            Command::RemoveFlag(f) => self.internal_flags.retain(|x| !f.contains(x)),
            Command::If {
                branches,
                otherwise,
            } => {
                let mut taken = false;
                for (test, block) in branches {
                    if self.eval_test(test)? {
                        self.run(block)?;
                        taken = true;
                        break;
                    }
                }
                if !taken && let Some(block) = otherwise {
                    self.run(block)?;
                }
            }
        }
        Ok(())
    }

    fn exec_redirect(&mut self, address: &str) {
        // Per-script amplification cap (engine side; the store adds a
        // per-account rate budget and loop guards).
        if self.redirects >= self.ctx.max_redirects {
            self.warnings
                .push("redirect count cap reached; redirect dropped".into());
            return;
        }
        // Never redirect to the owner's own address (trivial loop).
        if self.ctx.owns(address) {
            self.warnings.push("redirect to own address refused".into());
            return;
        }
        self.redirects += 1;
        self.actions.push(Action::Redirect {
            address: address.to_owned(),
        });
        // redirect does not cancel implicit keep (RFC 5228 §4.2).
    }

    fn exec_vacation(&mut self, v: &Vacation) {
        // RFC 3834 / RFC 5230 §4.x guard rails — decided from the message.
        // Null return path → never reply.
        let Some(return_path) = self.msg.envelope_from.clone() else {
            self.warnings
                .push("vacation: null return-path, no reply".into());
            return;
        };
        if return_path.trim().is_empty() {
            return;
        }
        // Auto-submitted (≠ no) → no reply.
        if let Some(v) = self.msg.header_values("Auto-Submitted").first()
            && !v.trim().eq_ignore_ascii_case("no")
        {
            self.warnings
                .push("vacation: Auto-Submitted, no reply".into());
            return;
        }
        // Mailing-list / bulk headers → no reply.
        for h in [
            "List-Id",
            "List-Post",
            "List-Unsubscribe",
            "List-Subscribe",
            "List-Help",
            "List-Owner",
        ] {
            if self.msg.has_header(h) {
                self.warnings.push("vacation: list mail, no reply".into());
                return;
            }
        }
        if let Some(p) = self.msg.header_values("Precedence").first() {
            let p = p.trim().to_ascii_lowercase();
            if p == "bulk" || p == "list" || p == "junk" {
                self.warnings
                    .push("vacation: bulk precedence, no reply".into());
                return;
            }
        }
        // Owner set = account addresses ∪ vacation :addresses.
        let mut owners = self.ctx.owner_addresses.clone();
        owners.extend(v.addresses.iter().map(|a| a.to_ascii_lowercase()));
        let is_owner = |addr: &str| owners.iter().any(|o| *o == addr.to_ascii_lowercase());
        // Never reply to ourselves (return path or From is us).
        if is_owner(&return_path) {
            self.warnings.push("vacation: from self, no reply".into());
            return;
        }
        if self
            .msg
            .header_addresses("From")
            .iter()
            .any(|a| is_owner(&a.all))
        {
            return;
        }
        // Only reply if the message was actually addressed to the owner
        // (RFC 5230 §4.5): To/Cc/envelope-to contains an owner address.
        let addressed_to_owner = self.msg.envelope_to_is(&owners)
            || self
                .msg
                .header_addresses("To")
                .iter()
                .chain(self.msg.header_addresses("Cc").iter())
                .any(|a| is_owner(&a.all));
        if !addressed_to_owner {
            self.warnings
                .push("vacation: not addressed to owner, no reply".into());
            return;
        }
        self.actions.push(Action::Vacation(VacationReply {
            to: return_path,
            subject: v.subject.clone(),
            from: v.from.clone(),
            handle: v.handle.clone(),
            days: v.days,
            reason: v.reason.clone(),
        }));
        // vacation does not cancel implicit keep.
    }

    fn eval_test(&mut self, test: &Test) -> Result<bool, EvalError> {
        self.tick()?;
        Ok(match test {
            Test::True => true,
            Test::False => false,
            Test::Not(t) => !self.eval_test(t)?,
            Test::AllOf(ts) => {
                for t in ts {
                    if !self.eval_test(t)? {
                        return Ok(false);
                    }
                }
                true
            }
            Test::AnyOf(ts) => {
                for t in ts {
                    if self.eval_test(t)? {
                        return Ok(true);
                    }
                }
                false
            }
            Test::Exists(headers) => headers.iter().all(|h| self.msg.has_header(h)),
            Test::Size { over, limit } => {
                if *over {
                    self.msg.size > *limit
                } else {
                    self.msg.size < *limit
                }
            }
            Test::Header {
                comparator,
                match_type,
                headers,
                keys,
            } => {
                let mut hit = false;
                'outer: for h in headers {
                    for value in self.msg.header_values(h) {
                        for key in keys {
                            self.tick()?;
                            if match_key(*comparator, *match_type, value, key) {
                                hit = true;
                                break 'outer;
                            }
                        }
                    }
                }
                hit
            }
            Test::Address {
                comparator,
                match_type,
                part,
                headers,
                keys,
            } => {
                let mut addrs = Vec::new();
                for h in headers {
                    addrs.extend(self.msg.header_addresses(h));
                }
                self.match_addresses(*comparator, *match_type, *part, &addrs, keys)?
            }
            Test::Envelope {
                comparator,
                match_type,
                part,
                fields,
                keys,
            } => {
                let mut addrs = Vec::new();
                for f in fields {
                    match f.to_ascii_lowercase().as_str() {
                        "from" => {
                            if let Some(a) =
                                self.msg.envelope_from.as_deref().and_then(Address::parse)
                            {
                                addrs.push(a);
                            }
                        }
                        "to" => {
                            if let Some(a) = Address::parse(&self.msg.envelope_to) {
                                addrs.push(a);
                            }
                        }
                        _ => {}
                    }
                }
                self.match_addresses(*comparator, *match_type, *part, &addrs, keys)?
            }
        })
    }

    fn match_addresses(
        &mut self,
        comparator: Comparator,
        match_type: MatchType,
        part: AddressPart,
        addrs: &[Address],
        keys: &[String],
    ) -> Result<bool, EvalError> {
        for a in addrs {
            let value = match part {
                AddressPart::All => Some(a.all.clone()),
                AddressPart::LocalPart => Some(a.local.clone()),
                AddressPart::Domain => Some(a.domain.clone()),
                AddressPart::User => Some(a.user_detail().0),
                AddressPart::Detail => a.user_detail().1,
            };
            let Some(value) = value else { continue };
            for key in keys {
                self.tick()?;
                if match_key(comparator, match_type, &value, key) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl Message {
    fn envelope_to_is(&self, owners: &[String]) -> bool {
        let to = self.envelope_to.to_ascii_lowercase();
        owners.contains(&to)
    }
}

/// Applies a comparator + match-type to one value/key pair.
fn match_key(comparator: Comparator, match_type: MatchType, value: &str, key: &str) -> bool {
    match match_type {
        MatchType::Is => compare_is(comparator, value, key),
        MatchType::Contains => contains(comparator, value, key),
        MatchType::Matches => glob(comparator, key, value),
    }
}

fn compare_is(comparator: Comparator, value: &str, key: &str) -> bool {
    match comparator {
        Comparator::Octet => value == key,
        Comparator::AsciiCasemap => value.eq_ignore_ascii_case(key),
        Comparator::AsciiNumeric => match (parse_leading_num(value), parse_leading_num(key)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        },
    }
}

fn contains(comparator: Comparator, value: &str, key: &str) -> bool {
    if key.is_empty() {
        return true;
    }
    match comparator {
        Comparator::Octet | Comparator::AsciiNumeric => value.contains(key),
        Comparator::AsciiCasemap => value
            .to_ascii_lowercase()
            .contains(&key.to_ascii_lowercase()),
    }
}

/// Leading-decimal parse for `i;ascii-numeric` (RFC 4790 §9.1.1): the value
/// is the leading run of digits; no digits → not a number.
fn parse_leading_num(s: &str) -> Option<u64> {
    let digits: String = s
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Linear-time glob (`*` any run, `?` one char, `\\` escapes), case per
/// comparator. No backtracking blowup: classic two-pointer with a single
/// star-anchor.
fn glob(comparator: Comparator, pattern: &str, text: &str) -> bool {
    let ci = matches!(comparator, Comparator::AsciiCasemap);
    let norm = |s: &str| {
        if ci {
            s.to_ascii_lowercase()
        } else {
            s.to_owned()
        }
    };
    let pat: Vec<char> = norm(pattern).chars().collect();
    let txt: Vec<char> = norm(text).chars().collect();
    glob_match(&pat, &txt)
}

/// A glob token: a literal, or a wildcard `*`/`?` (an escaped char is a
/// literal, losing wildcard meaning).
#[derive(PartialEq)]
enum G {
    Star,
    Any,
    Lit(char),
}

fn glob_match(pattern: &[char], text: &[char]) -> bool {
    // Build the token stream honoring `\` escapes.
    let mut toks: Vec<G> = Vec::with_capacity(pattern.len());
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            '\\' if i + 1 < pattern.len() => {
                toks.push(G::Lit(pattern[i + 1]));
                i += 2;
            }
            '*' => {
                toks.push(G::Star);
                i += 1;
            }
            '?' => {
                toks.push(G::Any);
                i += 1;
            }
            c => {
                toks.push(G::Lit(c));
                i += 1;
            }
        }
    }
    // Two-pointer with star backtracking (linear on realistic input).
    let (mut p, mut t) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match toks.get(p) {
            Some(G::Star) => {
                star = Some((p, t));
                p += 1;
            }
            Some(G::Any) => {
                p += 1;
                t += 1;
            }
            Some(G::Lit(c)) if *c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => {
                if let Some((sp, st)) = star {
                    p = sp + 1;
                    t = st + 1;
                    star = Some((sp, st + 1));
                } else {
                    return false;
                }
            }
        }
    }
    while matches!(toks.get(p), Some(G::Star)) {
        p += 1;
    }
    p == toks.len()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::parser::{Limits, compile};

    fn run(src: &str, raw: &[u8], from: Option<&str>, to: &str) -> Outcome {
        let script = compile(src, Limits::default()).expect("compile");
        let msg = Message::parse(raw, from.map(str::to_owned), to.to_owned());
        let ctx = EvalContext::new(vec![to.to_owned()]);
        evaluate(&script, &msg, &ctx).expect("eval")
    }

    const MSG: &[u8] =
        b"From: coyote@desert.test\r\nTo: bob@example.com\r\nSubject: $$$ money\r\n\r\nbody\r\n";

    #[test]
    fn implicit_keep_when_nothing_fires() {
        let out = run("keep;", MSG, Some("coyote@desert.test"), "bob@example.com");
        assert_eq!(out.actions, vec![Action::Keep { flags: vec![] }]);
    }

    #[test]
    fn header_contains_discards() {
        let out = run(
            "if header :contains \"subject\" \"$$$\" { discard; }",
            MSG,
            Some("x@y"),
            "bob@example.com",
        );
        // discard cancels implicit keep → no keep action.
        assert!(out.actions.is_empty());
    }

    #[test]
    fn fileinto_matches_glob() {
        let out = run(
            "require [\"fileinto\"]; if header :matches \"subject\" \"*money*\" { fileinto \"Money\"; }",
            MSG,
            Some("x@y"),
            "bob@example.com",
        );
        assert_eq!(
            out.actions,
            vec![Action::FileInto {
                mailbox: "Money".into(),
                flags: vec![]
            }]
        );
    }

    #[test]
    fn address_domain_test() {
        let out = run(
            "require [\"fileinto\"]; if address :domain :is \"from\" \"desert.test\" { fileinto \"Desert\"; }",
            MSG,
            Some("x@y"),
            "bob@example.com",
        );
        assert!(matches!(out.actions[0], Action::FileInto { .. }));
    }

    #[test]
    fn subaddress_detail() {
        let raw = b"From: a@b.test\r\nTo: bob+urgent@example.com\r\n\r\nx\r\n";
        let out = run(
            "require [\"fileinto\",\"subaddress\"]; if address :detail :is \"to\" \"urgent\" { fileinto \"Urgent\"; }",
            raw,
            Some("a@b.test"),
            "bob+urgent@example.com",
        );
        assert!(matches!(out.actions[0], Action::FileInto { .. }));
    }

    #[test]
    fn imap4flags_setflag_on_keep() {
        let out = run(
            "require [\"imap4flags\"]; setflag \"\\\\Seen\"; keep;",
            MSG,
            Some("x@y"),
            "bob@example.com",
        );
        assert_eq!(
            out.actions,
            vec![Action::Keep {
                flags: vec!["\\Seen".into()]
            }]
        );
    }

    #[test]
    fn redirect_to_self_refused_but_kept() {
        let out = run(
            "redirect \"bob@example.com\";",
            MSG,
            Some("x@y"),
            "bob@example.com",
        );
        // self-redirect dropped, implicit keep remains.
        assert_eq!(out.actions, vec![Action::Keep { flags: vec![] }]);
        assert!(!out.warnings.is_empty());
    }

    #[test]
    fn redirect_count_capped() {
        let src = "redirect \"a@x.test\"; redirect \"b@x.test\"; redirect \"c@x.test\"; redirect \"d@x.test\";";
        let out = run(src, MSG, Some("s@y"), "bob@example.com");
        let redirects = out
            .actions
            .iter()
            .filter(|a| matches!(a, Action::Redirect { .. }))
            .count();
        assert_eq!(redirects, 3, "capped at 3 redirects");
    }

    #[test]
    fn vacation_guards_list_mail() {
        let raw = b"From: sender@x.test\r\nTo: bob@example.com\r\nList-Id: <l.x.test>\r\n\r\nx\r\n";
        let out = run(
            "require [\"vacation\"]; vacation \"away\";",
            raw,
            Some("sender@x.test"),
            "bob@example.com",
        );
        assert!(!out.actions.iter().any(|a| matches!(a, Action::Vacation(_))));
    }

    #[test]
    fn vacation_replies_to_real_correspondent() {
        let raw = b"From: sender@x.test\r\nTo: bob@example.com\r\n\r\nx\r\n";
        let out = run(
            "require [\"vacation\"]; vacation :days 7 :subject \"Away\" \"I am away\";",
            raw,
            Some("sender@x.test"),
            "bob@example.com",
        );
        let v = out.actions.iter().find_map(|a| match a {
            Action::Vacation(v) => Some(v),
            _ => None,
        });
        let v = v.expect("vacation reply emitted");
        assert_eq!(v.to, "sender@x.test");
        assert_eq!(v.subject.as_deref(), Some("Away"));
        // implicit keep also present.
        assert!(out.files_somewhere());
    }

    #[test]
    fn budget_exhaustion_is_error() {
        let limits = Limits {
            max_instructions: 3,
            ..Limits::default()
        };
        let script = compile("if anyof (true,true,true,true,true) { keep; }", limits).unwrap();
        let msg = Message::parse(MSG, Some("x@y".into()), "bob@example.com".into());
        let ctx = EvalContext::new(vec!["bob@example.com".into()]);
        assert_eq!(
            evaluate(&script, &msg, &ctx),
            Err(EvalError::BudgetExceeded)
        );
    }
}
