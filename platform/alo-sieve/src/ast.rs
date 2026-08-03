//! The Sieve abstract syntax tree (RFC 5228 §8) plus the supported
//! extension actions/tests. Produced by the parser, consumed by the
//! evaluator; carries no source spans (compile errors are reported at
//! parse time).

/// A comparator (RFC 5228 §2.7.3, §9). Default is `i;ascii-casemap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Comparator {
    /// `i;ascii-casemap` — case-insensitive ASCII (the default).
    #[default]
    AsciiCasemap,
    /// `i;octet` — exact byte comparison.
    Octet,
    /// `i;ascii-numeric` (RFC 4790/2244) — compare as decimal numbers.
    AsciiNumeric,
}

/// A match type (RFC 5228 §2.7.1). Default is `:is`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchType {
    /// `:is` — exact equality (the default).
    #[default]
    Is,
    /// `:contains` — substring.
    Contains,
    /// `:matches` — glob (`*`/`?` with `\\` escaping).
    Matches,
}

/// The address part selected by an `address`/`envelope` test (RFC 5228
/// §2.7.4, RFC 5233). Default is `:all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddressPart {
    /// The whole `local@domain`.
    #[default]
    All,
    /// The local part before `@` (before any `+detail` under subaddress).
    LocalPart,
    /// The domain after `@`.
    Domain,
    /// `:user` — the local part with any `+detail` stripped (subaddress).
    User,
    /// `:detail` — the `+detail` part (subaddress); absent → no match.
    Detail,
}

/// A test (RFC 5228 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Test {
    /// `address [COMPARATOR] [ADDRESS-PART] [MATCH-TYPE] header-list key-list`.
    Address {
        comparator: Comparator,
        match_type: MatchType,
        part: AddressPart,
        headers: Vec<String>,
        keys: Vec<String>,
    },
    /// `envelope [COMPARATOR] [ADDRESS-PART] [MATCH-TYPE] env-list key-list`.
    Envelope {
        comparator: Comparator,
        match_type: MatchType,
        part: AddressPart,
        fields: Vec<String>,
        keys: Vec<String>,
    },
    /// `header [COMPARATOR] [MATCH-TYPE] header-list key-list`.
    Header {
        comparator: Comparator,
        match_type: MatchType,
        headers: Vec<String>,
        keys: Vec<String>,
    },
    /// `size :over`/`:under` limit.
    Size {
        /// `true` = `:over`, `false` = `:under`.
        over: bool,
        limit: u64,
    },
    /// `exists header-list`.
    Exists(Vec<String>),
    /// `allof (test-list)`.
    AllOf(Vec<Test>),
    /// `anyof (test-list)`.
    AnyOf(Vec<Test>),
    /// `not test`.
    Not(Box<Test>),
    /// `true`.
    True,
    /// `false`.
    False,
}

/// Optional IMAP flags on a filing action (RFC 5232). `None` when the
/// `:flags` tagged argument was absent (internal flags apply instead).
pub type FlagArg = Option<Vec<String>>;

/// `vacation` arguments (RFC 5230).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Vacation {
    /// `:days` suppression window (default handled by the evaluator).
    pub days: Option<u32>,
    /// `:subject` for the reply.
    pub subject: Option<String>,
    /// `:from` reply From.
    pub from: Option<String>,
    /// `:addresses` — additional owner addresses for the "is it to me" test.
    pub addresses: Vec<String>,
    /// `:handle` — scopes suppression independently of the reason text.
    pub handle: Option<String>,
    /// `:mime` — the reason is a full MIME entity (accepted; treated as body).
    pub mime: bool,
    /// The reply body/reason.
    pub reason: String,
}

/// A command (RFC 5228 §4, plus extension actions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `require ["ext", ...]` — validated at parse time.
    Require(Vec<String>),
    /// `if test block [elsif test block]* [else block]`.
    If {
        /// `(test, block)` for the `if` and each `elsif`, in order.
        branches: Vec<(Test, Vec<Command>)>,
        /// The optional trailing `else` block.
        otherwise: Option<Vec<Command>>,
    },
    /// `stop`.
    Stop,
    /// `keep [:flags flag-list]`.
    Keep(FlagArg),
    /// `discard`.
    Discard,
    /// `fileinto [:flags flag-list] "mailbox"`.
    FileInto { flags: FlagArg, mailbox: String },
    /// `redirect "address"`.
    Redirect(String),
    /// `vacation ...`.
    Vacation(Vacation),
    /// `setflag [flag-list]` — replaces internal flags.
    SetFlag(Vec<String>),
    /// `addflag [flag-list]`.
    AddFlag(Vec<String>),
    /// `removeflag [flag-list]`.
    RemoveFlag(Vec<String>),
}
