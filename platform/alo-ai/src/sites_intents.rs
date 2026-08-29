//! alo Sites' verbs (ADR 0058, AC.5) — the website as it is on the internet,
//! the draft behind it, and nothing that publishes by itself.
//!
//! This is the whole of what the Website agent may do, and the words a model
//! reads about it. The executors live beside Sites' routes in `alo-jmap`
//! (`sites_intents.rs`), through the asker's tenant-scoped store; the seven
//! verbs the old tool set already had keep their executors in that crate's
//! `agent_sites` module — the grounded answer, the editing pair and the
//! publish are that module's subject matter — and are dispatched from the
//! same place as the new reads, so the agent has one place to look.
//!
//! The five rules the old tool set was written around hold unchanged — each
//! is a mistake the wording exists to prevent:
//!
//! - **A question about the site is answered from what is on the internet.**
//!   `site_answer` reads the *published* site — never the draft: a visitor
//!   asking for the opening hours must be told what the page they can load
//!   says, not what somebody is halfway through writing.
//! - **Nothing a site verb writes is public.** A drafted page and a rewritten
//!   heading land in the draft and stay there. `site_publish` is the ONLY
//!   verb that makes anything public, it is a write, and it waits for the
//!   owner's tap (ADR 0047 §1).
//! - **An agent edits the words, never the wiring.** `site_page_edit`
//!   rewrites text at a position `site_page_read` handed it; a link's target,
//!   an image's blob, a form's id and a block of code are refused at the
//!   executor.
//! - **Nothing here invents a fact about the business.** A website is the one
//!   surface where an invented price or opening time is read by strangers and
//!   believed.
//! - **Translating the site is the owner's, and the agent only counts.**
//!   `site_translation_status` says which language is short how many pages;
//!   the translating itself stays on the website's Languages screen, approved
//!   page by page.
//!
//! What AC.5 adds is the site as a *business* subject: its pages as a list
//! (`site_pages`), where it stands on the internet (`site_status`), and what
//! visitors did on it — the order inbox (`site_orders`) and the services
//! offered for booking (`site_bookings`). Reads all four: an order is
//! confirmed on the orders screen, where the owner sees the customer they are
//! answering.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

const SITE_OPT: Arg = Arg::optional(
    "site",
    "text",
    "which site, by name or address — their only site when left out",
);

/// The verbs.
pub const SITES_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "site_answer",
        purpose: "What the user's website says ON THE INTERNET right now — the pages of the version that is actually published, the posts that are live, and the documents they put in the site's public knowledge. It reads nothing that is only a draft and changes nothing. Answer from the passages it returns and cite the page or post each one names; when it comes back with nothing published, say the site is not live yet rather than answering from anything else you can see.",
        effect: Effect::Read,
        args: &[
            Arg::required(
                "question",
                "text",
                "what to look for, in the user's own words",
            ),
            SITE_OPT,
        ],
        answers: &[
            "what does my site say about delivery",
            "are the opening hours on the website",
            "what is on the internet about our prices",
        ],
        preview: None,
        undo: None,
        // The published corpus has no owner-side route: it is the grounding
        // the public site answers from, which is why this verb adapts none.
        routes: &[],
    },
    IntentSpec {
        name: "site_pages",
        purpose: "Every page of the website as a list, in navigation order — each with its title, its address and whether it is the home page. It reads the draft's page list and changes nothing. This is the map of the site; reading what is ON one page is site_page_read.",
        effect: Effect::Read,
        args: &[SITE_OPT],
        answers: &[
            "what pages does the website have",
            "is there an about page",
            "how is the site organised",
        ],
        preview: None,
        undo: None,
        routes: &["/sites/{id}/pages"],
    },
    IntentSpec {
        name: "site_status",
        purpose: "Where the website stands — whether it is live on the internet or still a draft, its address, how many pages it has, and when it was last published with how many publishes before that. It reads; it changes nothing. This is the answer to \"is the site live\" — putting the draft live is site_publish, a separate approval.",
        effect: Effect::Read,
        args: &[SITE_OPT],
        answers: &[
            "is the website live",
            "when did we last publish the site",
            "what is the site's address",
        ],
        preview: None,
        undo: None,
        routes: &["/sites", "/sites/{id}", "/sites/{id}/publishes"],
    },
    IntentSpec {
        name: "site_orders",
        purpose: "The orders visitors placed on the website — how many are new, confirmed, fulfilled and cancelled, and the newest ones with who ordered, what it comes to and where each stands. It reads; it changes nothing. Amounts arrive as integer minor units with a readable amount beside each — repeat them as they arrived and keep currencies apart. Confirming, fulfilling or cancelling an order is done on the orders screen, where the owner sees the customer they are answering.",
        effect: Effect::Read,
        args: &[SITE_OPT],
        answers: &[
            "did any orders come in",
            "how many orders are waiting",
            "who ordered this week",
        ],
        preview: None,
        undo: None,
        routes: &["/sites/{id}/orders"],
    },
    IntentSpec {
        name: "site_bookings",
        purpose: "What the website offers visitors to book — each service with how long it takes, where it happens, when it is offered and whether it is taking bookings right now. It reads; it changes nothing. The appointments themselves land in the owner's own calendar, which is the Agenda's to read.",
        effect: Effect::Read,
        args: &[SITE_OPT],
        answers: &[
            "what can people book on the site",
            "is the consultation bookable",
            "how long is each appointment",
        ],
        preview: None,
        undo: None,
        routes: &["/sites/{id}/bookings"],
    },
    IntentSpec {
        name: "site_page_read",
        purpose: "ONE page as it stands in the DRAFT — its title, its search-engine title and description, and every block on it with the position, the type and the exact text you may rewrite. It changes nothing. This is where the position, the type and the pointer that site_page_edit needs come from: read the page before you propose an edit to it, and never work any of the three out for yourself.",
        effect: Effect::Read,
        args: &[
            Arg::required("page", "text", "the page, by its title or its address"),
            SITE_OPT,
        ],
        answers: &[
            "what is on the about page",
            "read me the home page draft",
            "what does the contact page say",
        ],
        preview: None,
        undo: None,
        routes: &["/sites/{id}/pages/{pid}"],
    },
    IntentSpec {
        name: "site_seo_review",
        purpose: "Go through every page of the draft and report what search engines will find missing — a page with no description, a description that is too long or too short, two pages sharing a title, a page with no heading on it, a picture with no alt text. It reports; it changes nothing. Report what it returns and nothing else: it counts what is on the pages, so never claim a position in anybody's results, a ranking, a keyword's difficulty or how much traffic a change would bring.",
        effect: Effect::Read,
        args: &[SITE_OPT],
        answers: &[
            "review the site for search engines",
            "what is missing for seo",
            "why would google not like our pages",
        ],
        preview: None,
        undo: None,
        // The review is computed over the page list here; no route serves it.
        routes: &[],
    },
    IntentSpec {
        name: "site_translation_status",
        purpose: "How far the site's OWN languages have got — for each language the site is set up in, how many of its pages already have a version written in that language and how many are still missing. It counts; it changes nothing and it translates nothing. You CANNOT translate anything: whole-site translation is something the user starts on the website's Languages screen, where every proposed page is shown beside its original and nothing is kept until they approve it — say that plainly and never say you translated, are translating, or will translate a page or a site.",
        effect: Effect::Read,
        args: &[SITE_OPT],
        answers: &[
            "is the french version ready",
            "which languages are missing pages",
            "how far is the translation",
        ],
        preview: None,
        undo: None,
        routes: &["/sites/{id}/translation-readiness"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "site_page_draft",
        purpose: "Write a NEW page into the site's draft — a heading, an opening line, and a block of text under each of its own subheadings. It only proposes; once approved the page is SAVED AS A DRAFT and is NOT on the internet: it appears when the user publishes, which is a separate approval they give. It never becomes the home page and never replaces an existing page.",
        effect: Effect::Write,
        args: &[
            Arg::required("title", "text", "what the page is called"),
            Arg::required("heading", "text", "the page's own headline"),
            Arg::optional(
                "slug",
                "text",
                "the address segment, lowercase letters, digits and hyphens — made from the title when left out",
            ),
            Arg::optional(
                "seo_description",
                "text",
                "the sentence search engines show",
            ),
            Arg::optional("intro", "text", "one line under the headline"),
            Arg::optional(
                "sections",
                "array",
                "each {\"heading\": text, \"body\": text}, the blocks of the page in order",
            ),
            SITE_OPT,
        ],
        answers: &[
            "draft a services page",
            "write a page about the workshop",
            "add a page for the summer opening times",
        ],
        preview: Some(
            "A page called {title} will be written into the website's draft — not on the internet until a publish is separately approved.",
        ),
        undo: None,
        routes: &["/sites/{id}/pages"],
    },
    IntentSpec {
        name: "site_page_edit",
        purpose: "Change the WORDS of a page that already exists, in the draft. It can retitle the page, set its search-engine title and description, and rewrite text that is already on it — one entry per piece of text, each naming the position, the type and the pointer site_page_read gave you. It cannot add, remove or reorder a block, and it cannot touch a link's target, an image, a form or any code on the page. It only proposes; every rewrite lands in the DRAFT and is NOT on the internet until the user publishes. State at least one change, and read the page first.",
        effect: Effect::Write,
        args: &[
            Arg::required("page", "text", "the page, by its title or its address"),
            Arg::optional("title", "text", "the page's new title"),
            Arg::optional("seo_title", "text", "the new search-engine title"),
            Arg::optional(
                "seo_description",
                "text",
                "the new search-engine description",
            ),
            Arg::optional(
                "copy",
                "array",
                "each {\"index\": number, \"type\": text, \"pointer\": text, \"text\": text (the complete new wording)} — position, type and pointer from site_page_read",
            ),
            SITE_OPT,
        ],
        answers: &[
            "rewrite the heading on the home page",
            "fix the wording on the about page",
            "set a better description for the contact page",
        ],
        preview: Some(
            "The wording of the {page} page will change in the website's draft — not on the internet until a publish is separately approved.",
        ),
        undo: None,
        routes: &["/sites/{id}/pages/{pid}/sections"],
    },
    IntentSpec {
        name: "site_publish",
        purpose: "Put the site's draft ON THE INTERNET, exactly as it stands. This is the ONLY verb that makes anything public, and everything waiting in the draft goes live together — including changes somebody else made and anything you drafted earlier in this conversation. It only proposes: nothing goes live until the user approves. Propose it only when the user asks for the site, a page or a change to go live; say what will become public in your own sentence, and never tell them anything is live until they have approved it.",
        effect: Effect::Write,
        args: &[SITE_OPT],
        answers: &[
            "publish the site",
            "put that page live",
            "make the change visible to visitors",
        ],
        preview: Some(
            "The website's whole draft goes on the internet — every page as it stands, including changes made by others.",
        ),
        undo: None,
        routes: &["/sites/{id}/publish"],
    },
];

/// The Sites routes deliberately without a verb, each with its reason.
///
/// The longest exclusion list of any module, because Sites is the app with
/// the most machinery behind one noun: building, theming, selling and wiring
/// a website are all screens where the owner sees what they are changing, and
/// an agent's contribution is the words and the questions — not the wiring,
/// not the money, and nothing that makes something public as a side effect.
pub const SITES_EXCLUDED: &[Excluded] = &[
    // -- making and shaping a site: the builder's own screens ---------------
    Excluded {
        route: "/sites/generate",
        why: "Generating a whole site runs the product's own model flow from a brief the owner writes on the builder screen; an agent's page work goes through the draft verbs.",
    },
    Excluded {
        route: "/sites/subdomain-check",
        why: "The builder's live address check while the owner types; the agent never picks a site's address.",
    },
    Excluded {
        route: "/sites/theme-presets",
        why: "A theme is chosen by looking at it; a look is not a sentence an agent should write.",
    },
    Excluded {
        route: "/sites/config",
        why: "Serves the screen its own feature flags and limits; nothing in it is an answer about the user's site.",
    },
    Excluded {
        route: "/sites/templates",
        why: "The template gallery is the screen's own picker, chosen by looking; the agent starts from what exists.",
    },
    Excluded {
        route: "/sites/templates/{id}",
        why: "One template's detail feeds the gallery's preview pane; picking one is visual.",
    },
    Excluded {
        route: "/sites/templates/{id}/preview",
        why: "Renders a template for the gallery's preview pane; a rendering is for eyes.",
    },
    Excluded {
        route: "/sites/invitations/{token}",
        why: "Accepting a collaborator invitation proves who holds the token; an agent holds nobody's invitation.",
    },
    Excluded {
        route: "/sites/{id}/theme",
        why: "Colours and type are chosen by looking at them on the theme screen; the agent edits words, never the look.",
    },
    Excluded {
        route: "/sites/{id}/images/{blob}",
        why: "Serves an image file to the editor; an agent neither uploads nor claims assets.",
    },
    Excluded {
        route: "/sites/{id}/passwords",
        why: "Protecting pages with passwords is an access decision the owner takes on the site's own settings, and a password must never pass through a model turn.",
    },
    Excluded {
        route: "/sites/{id}/collaborators",
        why: "Who may edit the site is an access decision the owner takes face to face with the list of people.",
    },
    Excluded {
        route: "/sites/{id}/collaborators/{user}",
        why: "Removing an editor is the same access decision as adding one; neither is an agent's.",
    },
    // -- pages: the parts of the editor that are not words ------------------
    Excluded {
        route: "/sites/{id}/pages/order",
        why: "Arranging the navigation is a drag the owner does while looking at the menu; a layout is visual.",
    },
    Excluded {
        route: "/sites/{id}/pages/{pid}/home",
        why: "Which page greets every visitor is a structural decision the owner takes on the page list; the drafted pages never claim it.",
    },
    Excluded {
        route: "/sites/{id}/pages/{pid}/preview",
        why: "Renders the draft page for the editor's preview pane; a rendering is for eyes.",
    },
    Excluded {
        route: "/sites/{id}/pages/{pid}/password",
        why: "A page's password is an access secret; secrets never pass through a model turn.",
    },
    Excluded {
        route: "/sites/{id}/pages/{pid}/sections/{index}",
        why: "Adding or removing one block changes the page's structure; the agent rewrites words at positions the read handed it and never restructures.",
    },
    Excluded {
        route: "/sites/{id}/pages/{pid}/sections/{index}/move",
        why: "Reordering blocks is a drag the owner does while looking at the page; a layout is visual.",
    },
    Excluded {
        route: "/sites/{id}/pages/{pid}/ai-edits",
        why: "The editor's own inline AI proposals, shown beside the page they change; the agent's edits go through site_page_edit where the same rules hold.",
    },
    Excluded {
        route: "/sites/{id}/pages/{pid}/palette",
        why: "The page palette is picked by looking at colours; the agent edits words, never the look.",
    },
    Excluded {
        route: "/sites/{id}/pages/{pid}/palette/{kind}/preview",
        why: "Renders a palette candidate for the picker; a rendering is for eyes.",
    },
    Excluded {
        route: "/sites/{id}/pages/{pid}/locales/{locale}",
        why: "Writing one page's version in another language is the translation flow, approved beside its original on the Languages screen; the agent only counts what is missing.",
    },
    Excluded {
        route: "/sites/{id}/pages/{pid}/locales/{locale}/preview",
        why: "Renders a translated page for the Languages screen's side-by-side view; a rendering is for eyes.",
    },
    Excluded {
        route: "/sites/{id}/translation-proposals",
        why: "Whole-site translation is proposed page by page beside the originals and approved on the Languages screen; a second, quieter path from one chat message is what A2.1b exists to prevent.",
    },
    // -- publishing machinery beyond the one verb ---------------------------
    Excluded {
        route: "/sites/{id}/unpublish",
        why: "Taking the whole site off the internet is a drastic step the owner takes on the site's own screen, seeing what disappears; an agent proposing it in passing is a risk with no question behind it.",
    },
    Excluded {
        route: "/sites/{id}/schedule",
        why: "Scheduling a future publish is set on the publish screen beside the clock it obeys; the agent's publish is the immediate one the owner approves now.",
    },
    Excluded {
        route: "/sites/{id}/schedule/{schedule}",
        why: "Cancelling a scheduled publish is done where it was set, beside the time it names.",
    },
    Excluded {
        route: "/sites/{id}/publishes/compare",
        why: "Comparing two versions is a side-by-side reading the owner does on the history screen; the differences are visual.",
    },
    Excluded {
        route: "/sites/{id}/publishes/{publish}/pages",
        why: "One old version's page list feeds the history screen's detail pane.",
    },
    Excluded {
        route: "/sites/{id}/publishes/{publish}/pages/{page}/preview",
        why: "Renders one old version of a page for the history screen; a rendering is for eyes.",
    },
    Excluded {
        route: "/sites/{id}/publishes/{publish}/restore",
        why: "Rolling the draft back to an old version overwrites everything drafted since; the owner does that on the history screen, seeing exactly what they give up.",
    },
    // -- visitors' numbers: the screens' own feeds --------------------------
    Excluded {
        route: "/sites/{id}/analytics",
        why: "Serves the analytics screen its own chart feed; a question about the figures is Insights' to answer from its catalog.",
    },
    Excluded {
        route: "/sites/{id}/heatmap",
        why: "A heatmap is a picture of where visitors clicked; a picture is for eyes.",
    },
    Excluded {
        route: "/sites/{id}/conversions",
        why: "Serves the conversions screen its own chart feed; a question about the figures is Insights' to answer.",
    },
    Excluded {
        route: "/sites/{id}/attribution",
        why: "Serves the attribution screen its own breakdown; a question about the figures is Insights' to answer.",
    },
    Excluded {
        route: "/sites/{id}/leads",
        why: "The lead links screen manages tracking links the owner mints and shares; minting one is a campaign decision, not a question.",
    },
    Excluded {
        route: "/sites/{id}/leads/{link}",
        why: "Editing or retiring a tracking link is done on the screen that shows where it is already in use.",
    },
    Excluded {
        route: "/sites/{id}/submissions",
        why: "Form submissions carry visitors' personal messages addressed to the owner, read on the submissions screen; they are not material for an agent's answers.",
    },
    Excluded {
        route: "/sites/{id}/submissions.csv",
        why: "The same personal submissions as a file download; a file is handed to the person, not to a model.",
    },
    Excluded {
        route: "/sites/{id}/submissions/{submission}/lead",
        why: "Handing a submission to the CRM as a lead is a judgement about a person, taken while reading what they wrote.",
    },
    Excluded {
        route: "/sites/{id}/forms/{form}/submissions/{submission}",
        why: "Deleting a visitor's submission is a data-protection act the owner performs seeing exactly whose words are erased.",
    },
    // -- the order inbox beyond the summary ---------------------------------
    Excluded {
        route: "/sites/{id}/orders.csv",
        why: "The order inbox as a file download; a file is handed to the person, not to a model.",
    },
    Excluded {
        route: "/sites/{id}/orders/{order}",
        why: "Confirming, fulfilling, cancelling or deleting an order is a promise to a customer, made on the orders screen where the owner sees who they are answering.",
    },
    // -- bookings machinery beyond the summary ------------------------------
    Excluded {
        route: "/sites/{id}/booking-sources",
        why: "Lists the calendars a service could attach to, for the setup screen's picker; wiring availability is the owner's.",
    },
    Excluded {
        route: "/sites/{id}/bookings/{booking}",
        why: "Changing a service's hours, questions or calendar changes what strangers can book; the owner does that on the setup screen, seeing the whole shape at once.",
    },
    // -- selling: catalogs, shop, tickets — money and stock -----------------
    Excluded {
        route: "/sites/shop-config/propose",
        why: "The shop-setup flow runs the product's own model proposal with the owner on the screen; the agent sells nothing.",
    },
    Excluded {
        route: "/sites/{id}/catalogs",
        why: "A catalog is the priced face of the business; creating one is done on the catalog screen where every price is the owner's own entry.",
    },
    Excluded {
        route: "/sites/{id}/catalogs/{catalog}",
        why: "Renaming or deleting a catalog changes what a published site sells; that is the owner's screen.",
    },
    Excluded {
        route: "/sites/{id}/catalogs/{catalog}/categories",
        why: "Catalog categories arrange what is for sale; arranging is done looking at the catalog.",
    },
    Excluded {
        route: "/sites/{id}/catalogs/{catalog}/categories/{category}",
        why: "Editing a category is the same arranging, one entry at a time.",
    },
    Excluded {
        route: "/sites/{id}/catalogs/{catalog}/items",
        why: "A catalog item carries a price a stranger will pay; a price never comes from a model.",
    },
    Excluded {
        route: "/sites/{id}/catalogs/{catalog}/items/{item}",
        why: "Editing an item edits its price and availability; both are the owner's own entries.",
    },
    Excluded {
        route: "/sites/{id}/shop-settings",
        why: "How the shop takes money is wiring between the site and the payment account; wiring is not words.",
    },
    Excluded {
        route: "/sites/{id}/shop-products",
        why: "Publishing products into the paid shop puts prices in front of strangers; the owner does that from the shop screen.",
    },
    Excluded {
        route: "/sites/{id}/shop-items",
        why: "The shop's sellable items each carry a price and a stock link; both are the owner's own entries.",
    },
    Excluded {
        route: "/sites/{id}/shop-items/{item}",
        why: "Editing a sellable item edits its price; a price never comes from a model.",
    },
    Excluded {
        route: "/sites/{id}/ticket-products",
        why: "Ticket types carry prices and capacities for real events; both are the owner's own entries.",
    },
    Excluded {
        route: "/sites/{id}/tickets",
        why: "The ticket desk lists paid admissions with buyers' names; selling and refunding are done there.",
    },
    Excluded {
        route: "/sites/{id}/tickets/{event}",
        why: "One event's admissions are handled at the ticket desk, buyer by buyer.",
    },
    // -- collections: structured content with its own screen ----------------
    Excluded {
        route: "/sites/{id}/collections",
        why: "A collection's schema decides what the site can list; changing a schema is structure, not words.",
    },
    Excluded {
        route: "/sites/{id}/collections/{collection}",
        why: "Editing collection entries is done on the collection screen where the schema shows what each field means.",
    },
    Excluded {
        route: "/sites/{id}/collections/{collection}/preview",
        why: "Renders a collection for the editor's preview pane; a rendering is for eyes.",
    },
    // -- posts: their own editor -------------------------------------------
    Excluded {
        route: "/sites/{id}/posts",
        why: "Posts have their own editor and their own per-post publish; the agent's page work stays on pages, where the editing rules are enforced.",
    },
    Excluded {
        route: "/sites/{id}/posts/{post}",
        why: "Editing a post is the post editor's; its body is one blob the wiring rules cannot police.",
    },
    Excluded {
        route: "/sites/{id}/posts/{post}/publish",
        why: "Publishing a post puts it on the internet by itself; the one public-making verb an agent has is site_publish, and adding a second would blur it.",
    },
    Excluded {
        route: "/sites/{id}/posts/{post}/unpublish",
        why: "Taking a post off the internet is decided where it can be read.",
    },
    // -- the site's own chat assistant: configured, not conversed with ------
    Excluded {
        route: "/sites/{id}/chat-settings",
        why: "Turning the public site's own assistant on and shaping it is site configuration the owner does on its screen.",
    },
    Excluded {
        route: "/sites/{id}/chat-actions",
        why: "What the public assistant may do for strangers is a permission decision, taken where each action is explained.",
    },
    Excluded {
        route: "/sites/{id}/chat-appearance",
        why: "The public assistant's look is chosen by looking at it.",
    },
    Excluded {
        route: "/sites/{id}/chat-appearance/preview",
        why: "Renders the assistant's look for the picker; a rendering is for eyes.",
    },
    Excluded {
        route: "/sites/{id}/chat-knowledge",
        why: "What the public assistant is allowed to know is a disclosure decision: every source is added by the owner seeing what strangers will be told.",
    },
    Excluded {
        route: "/sites/{id}/chat-knowledge/{source}",
        why: "Removing a knowledge source is the same disclosure decision in reverse.",
    },
    // -- domains: contracts and money ---------------------------------------
    Excluded {
        route: "/sites/domain-catalog",
        why: "The domain shop's price list feeds the purchase screen; buying names is a contract flow.",
    },
    Excluded {
        route: "/sites/domain-search",
        why: "Searching purchasable names feeds the purchase screen's picker.",
    },
    Excluded {
        route: "/sites/domain-payments/settle",
        why: "Settles a domain payment with the payment provider; money moves, and money never moves from a model turn.",
    },
    Excluded {
        route: "/sites/{id}/domains",
        why: "Connecting a domain is DNS wiring between registrar and site; wiring is not words.",
    },
    Excluded {
        route: "/sites/{id}/domains/{domain}",
        why: "Disconnecting a domain takes a live address off the site; the owner does that seeing what breaks.",
    },
    Excluded {
        route: "/sites/{id}/domains/{domain}/verify",
        why: "Verifying DNS is a step of the connect flow, done on its screen against the records just edited.",
    },
    Excluded {
        route: "/sites/{id}/domain-purchases",
        why: "Buying a domain is a purchase with a registrant's legal details; a contract is entered on its own screen.",
    },
    Excluded {
        route: "/sites/{id}/domain-purchases/{purchase}",
        why: "One purchase's standing feeds the purchase screen it was started on.",
    },
    Excluded {
        route: "/sites/{id}/domain-purchases/{purchase}/registrant",
        why: "The registrant's legal identity is entered by the person it names.",
    },
    Excluded {
        route: "/sites/{id}/domain-purchases/{purchase}/approve",
        why: "Approving a purchase commits money; money never moves from a model turn.",
    },
    Excluded {
        route: "/sites/{id}/domain-purchases/{purchase}/checkout",
        why: "Checkout hands the purchase to the payment provider; money never moves from a model turn.",
    },
    Excluded {
        route: "/sites/{id}/domain-purchases/{purchase}/cancel",
        why: "Cancelling a purchase is done on the purchase screen that shows what has already been paid.",
    },
];

/// The Sites paragraph of the agent's general instructions.
///
/// It says what the *product* is, not what a verb takes: the sentence that
/// stops a model publishing by implication, the one that stops it filling a
/// stranger's screen with facts nobody at the company ever stated, and — new
/// with the order inbox — the one that keeps a visitor's money in the figures
/// it arrived in.
pub const SITES_GUIDANCE: &str = "For a website tool, NEVER invent a fact about the business: a price, an address, an opening time, a phone number, a delivery time, a statistic, a certification, a person or a customer's words are the user's own, and a website is read by strangers who will believe whatever is on it — ask for anything you have not been given rather than filling the gap. A page you draft or a wording you change is in the DRAFT: never say a change is live, online, updated or visible to anybody until the user has approved a publish, and say plainly that it is waiting for them. Write in the language of the page you are working on. When the user asks what their site says, read the published site rather than the draft, and cite the page you found it on. An order's amounts are the store's own figures — repeat them as they arrived, never add them up or convert them, and keep currencies apart; an order is confirmed or cancelled on the orders screen, not by you. Translating the site is not yours to do: site_translation_status tells you which language is short how many pages, and the translating itself is something the user runs from the website's Languages screen and approves page by page — offer the count and point them there rather than offering to do it.\n";

/// The module, as the registry reads it.
pub static SITES: IntentModule = IntentModule {
    intents: SITES_INTENTS,
    excluded: SITES_EXCLUDED,
    guidance: SITES_GUIDANCE,
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The verbs whose surface is not a route of this module — the published
    /// corpus is the public site's grounding with no owner-side route, and
    /// the review is computed over the page list here. Named, so a new verb
    /// with an empty route list fails the test instead of joining them
    /// silently.
    const ROUTELESS: &[&str] = &["site_answer", "site_seo_review"];

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in SITES_INTENTS {
            assert!(
                !intent.routes.is_empty() || ROUTELESS.contains(&intent.name),
                "{} names no route",
                intent.name
            );
            assert!(
                intent.purpose.ends_with('.'),
                "{} purpose is not a sentence",
                intent.name
            );
            assert!(
                !intent.answers.is_empty(),
                "{} answers nothing",
                intent.name
            );
            if intent.effect == Effect::Write {
                assert!(
                    intent.preview.is_some(),
                    "{} is a write without a preview",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn names_are_unique_and_the_doc_lists_each_once() {
        let mut names: Vec<&str> = SITES_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SITES_INTENTS.len());
        let doc = SITES.doc();
        for intent in SITES_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(SITES_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in SITES_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !SITES_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    /// Every read says, where the model reads it, that it changes nothing —
    /// so a turn never offers a button for an answer.
    #[test]
    fn the_reads_say_they_change_nothing() {
        for intent in SITES_INTENTS.iter().filter(|i| i.effect == Effect::Read) {
            assert!(
                intent.purpose.contains("changes nothing"),
                "{} does not say it changes nothing",
                intent.name
            );
        }
    }

    /// The sentence the whole module is named after: publishing is ONE verb,
    /// declared a write, and every other write says where its result actually
    /// lands — so a model that read only this block still cannot believe it
    /// put something on the internet.
    #[test]
    fn publishing_is_one_verb_that_waits_and_every_other_write_says_it_did_not_publish() {
        let publish = SITES.find("site_publish").unwrap();
        assert_eq!(publish.effect, Effect::Write);
        assert!(
            publish
                .purpose
                .contains("ONLY verb that makes anything public")
        );
        assert!(
            publish
                .purpose
                .contains("nothing goes live until the user approves")
        );
        for name in ["site_page_draft", "site_page_edit"] {
            let write = SITES.find(name).unwrap();
            assert_eq!(write.effect, Effect::Write);
            assert!(
                write.purpose.contains("NOT on the internet"),
                "{name} does not say where it lands"
            );
            assert!(
                write.preview.unwrap().contains("not on the internet"),
                "{name}'s preview does not say where it lands"
            );
        }
        assert!(SITES_GUIDANCE.contains("never say a change is live"));
        // …and no verb anywhere else in the set could make a post or a page
        // public: one public-making path, held structurally.
        for intent in SITES_INTENTS {
            assert!(
                intent.name == "site_publish" || !intent.name.contains("publish"),
                "{} would be a second publish path",
                intent.name
            );
        }
    }

    /// A question about the site is answered from what a visitor can load,
    /// and the purpose says so — the draft is not the site.
    #[test]
    fn the_answering_verb_reads_the_published_site_and_not_the_draft() {
        let answer = SITES.find("site_answer").unwrap();
        assert!(answer.purpose.contains("ON THE INTERNET"));
        assert!(answer.purpose.contains("nothing that is only a draft"));
        assert!(answer.purpose.contains("say the site is not live yet"));
        assert!(SITES_GUIDANCE.contains("read the published site rather than the draft"));
    }

    /// The editing pair, stated in the prompt: positions and pointers come
    /// from the read, never from the model, and the wiring is named as out of
    /// reach. A guessed index is how an edit lands on the wrong block.
    #[test]
    fn an_edit_names_a_position_the_read_verb_gave_it() {
        let read = SITES.find("site_page_read").unwrap();
        assert!(
            read.purpose
                .contains("never work any of the three out for yourself")
        );
        let edit = SITES.find("site_page_edit").unwrap();
        assert!(edit.purpose.contains("site_page_read gave you"));
        assert!(edit.purpose.contains("read the page first"));
        assert!(edit.purpose.contains("cannot touch a link's target"));
        assert!(
            edit.purpose
                .contains("cannot add, remove or reorder a block")
        );
    }

    /// The one mistake a website makes that no reviewer downstream catches: a
    /// fact nobody at the company ever stated, published for strangers to
    /// read. The drafting verb's arguments carry headings and prose and
    /// nothing that would hold a claim in structured form.
    #[test]
    fn nothing_here_offers_the_model_a_fact_to_make_up() {
        assert!(SITES_GUIDANCE.contains("NEVER invent a fact about the business"));
        for named in ["a price", "an address", "an opening time", "a statistic"] {
            assert!(SITES_GUIDANCE.contains(named), "{named}");
        }
        let draft = SITES.find("site_page_draft").unwrap();
        for arg in draft.args {
            assert!(
                !["price", "tiers", "testimonials", "members", "image"].contains(&arg.name),
                "{} would carry an invented claim",
                arg.name
            );
        }
    }

    /// The item A2.1b named: the agent reports how far the languages got and
    /// cannot translate — and no verb anywhere in the set is a second
    /// translation path.
    #[test]
    fn the_translation_verb_counts_and_says_it_cannot_translate() {
        let status = SITES.find("site_translation_status").unwrap();
        assert_eq!(status.effect, Effect::Read);
        assert!(status.purpose.contains("how many are still missing"));
        assert!(status.purpose.contains("CANNOT translate anything"));
        assert!(status.purpose.contains("Languages screen"));
        assert!(
            status.purpose.contains("never say you translated"),
            "the tense a model reaches for first is the one to forbid"
        );
        assert!(SITES_GUIDANCE.contains("Translating the site is not yours to do"));
        for intent in SITES_INTENTS {
            assert!(
                !intent.name.contains("translate"),
                "{} would be a second translation path",
                intent.name
            );
        }
    }

    /// A review reports what is on the page. Everything else a search-engine
    /// verb could be asked for — a ranking, a position, traffic — is a claim
    /// about somebody else's index that we cannot see and would not be true.
    #[test]
    fn the_review_never_claims_a_ranking() {
        let review = SITES.find("site_seo_review").unwrap();
        assert!(review.purpose.contains("never claim a position"));
        assert!(review.purpose.contains("how much traffic"));
    }

    /// AC.5's own additions are reads, and the order inbox stays one: the
    /// money a visitor committed is repeated, never recomputed, and the
    /// order's standing is changed on the screen that shows the customer.
    #[test]
    fn the_business_reads_answer_and_never_touch_an_order() {
        for name in ["site_pages", "site_status", "site_orders", "site_bookings"] {
            let read = SITES.find(name).unwrap();
            assert_eq!(read.effect, Effect::Read, "{name} must be a read");
        }
        let orders = SITES.find("site_orders").unwrap();
        assert!(orders.purpose.contains("repeat them as they arrived"));
        assert!(orders.purpose.contains("orders screen"));
        assert!(SITES_GUIDANCE.contains("repeat them as they arrived"));
        assert!(SITES_GUIDANCE.contains("keep currencies apart"));
        // No verb confirms, fulfils, cancels or deletes anything a visitor
        // sent — those routes are excluded with their reasons.
        for intent in SITES_INTENTS {
            for verb in ["confirm", "fulfil", "cancel", "delete"] {
                assert!(
                    !intent.name.contains(verb),
                    "{} would answer a customer without the owner",
                    intent.name
                );
            }
        }
    }
}
