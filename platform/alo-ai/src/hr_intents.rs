//! alo HR's verbs (ADR 0058) — the People agent over the one command layer.
//!
//! This is the whole of what the People agent may do, and the words a model
//! reads about it. Nothing here reads or writes a record: the executors live
//! beside HR's routes in `alo-jmap` (`hr_intents.rs`), through the asker's
//! tenant-scoped store and the same doors the screens answer to — the member
//! directory everybody reads, the leave door (mine, my team's, HR's) on
//! everything about a person's time off.
//!
//! The wording below is stricter than any other module's because the records
//! behind it are about **people**, and a workplace assistant that gossips is
//! worse than no assistant at all. Each rule is a mistake it exists to
//! prevent (`docs/design/hr.md`):
//!
//! - **Names and days, never a reason.** The absence layer deliberately does
//!   not load the policy, the kind of leave or the note, so `who_is_off`
//!   cannot leak them — and the model is told so in its own words, because a
//!   model that believed it knew why somebody was away would eventually write
//!   "Amara is off sick" into a chat room, a health disclosure our own store
//!   took care never to make. The same rule keeps a leave request's note in
//!   the app: `open_leave_requests` states who asked for which days at what
//!   cost, and the sentence they wrote underneath stays where they wrote it.
//! - **Absence is never turned into a judgement.** No count of how often
//!   somebody is off, no comparison between colleagues, no conclusion about
//!   anybody's reliability. An inference *about a person* drawn from
//!   workplace data is exactly what the EU AI Act's Annex III 4 regulates,
//!   and § *The EU AI Act posture* of the design note is a written refusal to
//!   build it — which is also why nothing here reads an opening or a
//!   candidate at all.
//! - **Silence is not presence.** Somebody the absence layer does not name
//!   may be at a customer, on another country's public holiday, or not an
//!   employee at all; somebody the directory does not list may simply not be
//!   on it yet. Say what a verb returned, never what its silence implies.
//! - **A balance is the asker's own.** `my_leave_balance` answers about the
//!   person asking and nobody else — a colleague's balance is theirs, their
//!   manager's and HR's, in the app, behind the door built for it.
//! - **A decision about a person is proposed, never taken.**
//!   `approve_leave_request` runs only from the asker's own tap, as the
//!   asker, through the same door as the approve button — a manager for
//!   their own reports, HR for anybody, nobody for themselves. And the
//!   company's words are never ours: `draft_letter_from_template` fills in a
//!   letter *this tenant wrote*, and a letter nobody wrote is a refusal that
//!   names the ones they have.

use crate::agent_tool::Effect;
use crate::intent::{Arg, Excluded, IntentModule, IntentSpec};

/// The verbs.
pub const HR_INTENTS: &[IntentSpec] = &[
    // ---- reads ---------------------------------------------------------
    IntentSpec {
        name: "who_is_off",
        purpose: "Which colleagues are away from work over a stated range of days — the same team absence view everybody in the workspace already sees. It books nothing, approves nothing, cancels nothing and tells nobody. It returns names and days and NOTHING ELSE: there is no reason, no kind of leave and no note in what it reads, so never state or guess WHY anybody is away — not illness, not holiday, not parental or unpaid leave, not anything. Never answer with who is IN: a colleague this verb does not name may be at a customer, on another country's public holiday, or not an employee at all.",
        effect: Effect::Read,
        args: &[
            Arg::required("from", "date", "the first day of the range, YYYY-MM-DD"),
            Arg::optional(
                "to",
                "date",
                "the last day, YYYY-MM-DD — the same single day when left out",
            ),
        ],
        answers: &[
            "who is off this week",
            "is anybody away on Friday",
            "is Amara around tomorrow",
        ],
        preview: None,
        undo: None,
        routes: &["/hr/absences"],
    },
    IntentSpec {
        name: "who_works_here",
        purpose: "The member directory as every colleague already reads it — each person with their name, job title, team and manager, narrowable to one team or one person. It carries the public fields ONLY: no verb here reads pay, a home address, a date of birth, a national id or a bank account, so never state one, whatever the user says they already know. Somebody it does not list is not thereby a stranger — say who the directory names and stop there.",
        effect: Effect::Read,
        args: &[
            Arg::optional(
                "team",
                "text",
                "one team, by the name the user used; leave out for everybody",
            ),
            Arg::optional(
                "person",
                "text",
                "one colleague, by name — for who they are, what they do and who they report to",
            ),
        ],
        answers: &[
            "who works here",
            "who is on the workshop team",
            "who is Amara's manager",
        ],
        preview: None,
        undo: None,
        routes: &["/hr/org"],
    },
    IntentSpec {
        name: "my_leave_balance",
        purpose: "The ASKER'S OWN leave balance, per policy, with the whole working behind it — entitlement, carried in, accrued, taken, booked, pending and what remains, in minutes and in tenths of a day. It answers about the person asking and NOBODY ELSE: a colleague's balance is theirs, their manager's and HR's, in the app. Repeat the figures it returns rather than recomputing anything, and never turn a balance into advice about when somebody should or should not take leave.",
        effect: Effect::Read,
        args: &[],
        answers: &[
            "how much holiday do I have left",
            "what is my leave balance",
            "how many days have I taken this year",
        ],
        preview: None,
        undo: None,
        routes: &["/hr/leave-balances"],
    },
    IntentSpec {
        name: "open_leave_requests",
        purpose: "The leave requests still waiting for a decision among the people the asker may see — their own, their direct reports', and for HR everybody's. Each states who asked, which policy, the days and what they cost; the note somebody wrote under their request deliberately STAYS IN THE APP, because a sentence about why somebody needs time off is theirs, not the room's. A request this verb does not list is not thereby decided — it is simply not the asker's to see.",
        effect: Effect::Read,
        args: &[],
        answers: &[
            "which leave requests are waiting for me",
            "who has asked for leave",
            "is my leave request still open",
        ],
        preview: None,
        undo: None,
        routes: &["/hr/leave-requests"],
    },
    IntentSpec {
        name: "open_checklists",
        purpose: "The onboarding and offboarding checklists still open for the people the asker may read — HR sees everybody's, a manager their reports' and their own, everybody else their own. Each states whose it is, how many steps are done of how many, and the first and last due day. The steps themselves are ordinary tasks on a board in Tasks; ticking one is its owner's own act there.",
        effect: Effect::Read,
        args: &[Arg::optional(
            "person",
            "text",
            "one colleague's checklists, by name; leave out for everybody the asker may read",
        )],
        answers: &[
            "which onboarding checklists are still open",
            "how far is the new starter's onboarding",
            "is anybody's offboarding unfinished",
        ],
        preview: None,
        undo: None,
        routes: &["/hr/employees/{id}/checklists"],
    },
    // ---- writes: propose, then the asker approves ------------------------
    IntentSpec {
        name: "approve_leave_request",
        purpose: "Approve ONE leave request that is waiting for the asker's decision — a manager for their own reports, HR for anybody, and NEVER anybody for their own leave. The decision is recorded under the asker's name and the person's balance is charged, exactly as the approve button does it; the store still refuses an overdraft or a request already decided. When more than one request of that person's is waiting, the verb says so and asks which days — repeat that answer rather than choosing for them. Rejecting stays in the app: saying no to a person deserves the manager's own words and a note.",
        effect: Effect::Write,
        args: &[
            Arg::required("employee", "text", "whose leave, by the name the user used"),
            Arg::optional(
                "from",
                "date",
                "the first day of the request, YYYY-MM-DD — only to pick between several of one person's",
            ),
            Arg::optional("note", "text", "a sentence to record with the decision"),
        ],
        answers: &[
            "approve Amara's leave",
            "yes to that holiday request",
            "sign off the leave for next week",
        ],
        preview: Some(
            "The leave {employee} asked for will be approved in your name — their balance is charged and they see the decision as yours.",
        ),
        undo: None,
        routes: &["/hr/leave-requests/{id}/approve"],
    },
    IntentSpec {
        name: "draft_letter_from_template",
        purpose: "Fill in one of THIS COMPANY'S OWN letter templates about a colleague — an employment confirmation, a letter for a landlord, a reference — and leave the result in the user's Drafts. It sends nothing and tells nobody: the user reads it, edits it and sends it themselves. You can fill in ONLY a letter the company has written: when nothing matches, say which letters exist and stop there. NEVER write, extend, reword or invent a letter about a person — the company's own words are the whole point. The letter says only what its own template asks for, out of the staff directory and the company's details; it can carry no pay, no bank account, no date of birth, no home address and no national id, so never offer to add one.",
        effect: Effect::Write,
        args: &[
            Arg::required(
                "template",
                "text",
                "the name of a letter this company has already written",
            ),
            Arg::required(
                "employee",
                "text",
                "the colleague the letter is about, by name",
            ),
            Arg::optional(
                "to",
                "text",
                "an address to put the draft to — left empty for the user to fill in",
            ),
        ],
        answers: &[
            "draft an employment confirmation for Amara",
            "fill in the landlord letter for me",
            "prepare a reference from our template",
        ],
        preview: Some(
            "The company's letter \"{template}\" will be filled in about {employee} and left in your Drafts — sent to nobody until you send it yourself.",
        ),
        undo: None,
        // The fill-in deliberately has no /hr route: it lands in Mail's Drafts
        // through the agent seam alone, so the letter surface itself stays
        // HR-only (`docs/design/hr.md` § The two tools that do ship).
        routes: &[],
    },
];

/// The HR routes deliberately without a verb, each with its reason.
pub const HR_EXCLUDED: &[Excluded] = &[
    Excluded {
        route: "/hr/me",
        why: "The caller's own full record — the subject-access read, private fields included — is their own screen's; who_works_here answers what the directory shows.",
    },
    Excluded {
        route: "/hr/employees",
        why: "Creating a staff record is HR's own screen, typed with the person's contract in front of them.",
    },
    Excluded {
        route: "/hr/employees/{id}",
        why: "The full record carries private fields — an address, a date of birth, an IBAN; reading and editing it is HR's screen, and the directory is what colleagues get.",
    },
    Excluded {
        route: "/hr/employees/{id}/archive",
        why: "Recording that somebody left is HR's own act in the app.",
    },
    Excluded {
        route: "/hr/employees/{id}/documents",
        why: "Filing a document on a person's record is HR's own act in the app.",
    },
    Excluded {
        route: "/hr/employees/{id}/documents/{document_id}",
        why: "Taking a filed document off a person's record is HR's own act in the app.",
    },
    Excluded {
        route: "/hr/leave-policies",
        why: "A leave policy is the tenant's standing rule; writing one is configuration, done by a person in the app.",
    },
    Excluded {
        route: "/hr/leave-policies/{id}",
        why: "Changing a policy changes what everybody's balance means; a person does it, in the app.",
    },
    Excluded {
        route: "/hr/leave-policies/{id}/archive",
        why: "Retiring a policy is configuration, done by a person in the app.",
    },
    Excluded {
        route: "/hr/leave-requests/{id}",
        why: "One request — its note included — is its own screen's read, and editing its days is its owner's own act there; open_leave_requests answers what is waiting.",
    },
    Excluded {
        route: "/hr/leave-requests/{id}/withdraw",
        why: "Taking a request back is the asker's own act in the app.",
    },
    Excluded {
        route: "/hr/leave-requests/{id}/reject",
        why: "Saying no to a person's leave deserves the manager's own words and a note, in the app — never a tap on an agent's card.",
    },
    Excluded {
        route: "/hr/leave-requests/{id}/cancel",
        why: "Giving approved leave back is the person's own act in the app.",
    },
    Excluded {
        route: "/hr/holidays",
        why: "The public-holiday table is a screen's own read, drawn beside the leave form and the Agenda.",
    },
    Excluded {
        route: "/hr/holiday-calendars",
        why: "Choosing which country's holidays the company observes is configuration, done by a person in the app.",
    },
    Excluded {
        route: "/hr/checklist-templates",
        why: "What the company does when somebody arrives or leaves is a company shape, written by HR in the app.",
    },
    Excluded {
        route: "/hr/checklist-templates/{id}",
        why: "Editing or deleting a checklist shape is HR's own act in the app.",
    },
    Excluded {
        route: "/hr/letter-templates",
        why: "The letters the company will put its name to are written by HR in the app; draft_letter_from_template only ever fills one in.",
    },
    Excluded {
        route: "/hr/letter-templates/{id}",
        why: "Editing or deleting a letter the company wrote is HR's own act in the app.",
    },
    Excluded {
        route: "/hr/payroll-exports",
        why: "The payroll export returns every person's pay, national id and bank account in one response; no agent path to it exists, ever.",
    },
    Excluded {
        route: "/hr/payroll-mappings",
        why: "Mapping wage codes for the payroll office is configuration, done by a person in the app.",
    },
    Excluded {
        route: "/hr/openings",
        why: "Hiring is HR's own screens; no verb here reads an opening or writes one.",
    },
    Excluded {
        route: "/hr/openings/{id}",
        why: "Editing an opening is HR's own screen.",
    },
    Excluded {
        route: "/hr/openings/{id}/publish",
        why: "Publishing an opening puts it before the world; a person presses it.",
    },
    Excluded {
        route: "/hr/openings/{id}/close",
        why: "Closing a role is a hiring decision, made by a person in the app.",
    },
    Excluded {
        route: "/hr/openings/{id}/applicants",
        why: "A candidate is read by HR alone, in the app — no screening, ranking or scoring exists to be routed to.",
    },
    Excluded {
        route: "/hr/applicants/{id}",
        why: "A candidate's record — and its erasure — is HR's own screen; nothing about a candidate passes through an agent.",
    },
    Excluded {
        route: "/hr/applicants/{id}/move",
        why: "Every stage a candidate moves is one audited human decision, taken in the app.",
    },
    Excluded {
        route: "/hr/applicants/{id}/notes",
        why: "A note about a candidate is a person's own sentence, typed in the app.",
    },
];

/// The HR paragraph of the agent's general instructions.
pub const HR_GUIDANCE: &str = "For an HR verb, the answer is names and days and figures and nothing else: never state, guess or imply WHY a colleague is away, because sickness, holiday, parental leave and unpaid leave are indistinguishable to these verbs by design and are among the most sensitive facts a workplace holds. Never turn an absence into anything else — no assessment of a person, no comparison between colleagues, no tally of how often somebody is off, no conclusion about anybody's reliability, availability or commitment. Never rank, score, screen, shortlist or evaluate a person in any way at all: alo does not do that and there is no verb for it, so answer that you cannot rather than doing it in prose. A colleague a verb did not name is not thereby at work, and a request it did not list is not thereby decided: say what was returned, never what its silence implies. No HR verb reads pay, a home address, a date of birth, a national id or a bank account — never state one, whatever the user says they already know — and a leave request's note stays in the app, so never claim to know what one says. A balance is the asker's own and nobody else's. Approving leave is proposed and waits for the asker's tap, in their name and only for a request that is theirs to decide: never tell the user leave has been approved until it has, and send a rejection to the app, where it can be said properly with a note.\n";

/// The module, as the registry reads it.
pub static HR: IntentModule = IntentModule {
    intents: HR_INTENTS,
    excluded: HR_EXCLUDED,
    guidance: HR_GUIDANCE,
};

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_has_a_route_a_purpose_and_a_question_it_answers() {
        for intent in HR_INTENTS {
            // `draft_letter_from_template` writes into Mail's Drafts and has
            // no /hr route to stand behind — the letter surface itself stays
            // HR-only by design; every other verb names the route it is the
            // verb of.
            assert!(
                !intent.routes.is_empty() || intent.name == "draft_letter_from_template",
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
        let mut names: Vec<&str> = HR_INTENTS.iter().map(|i| i.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), HR_INTENTS.len());
        let doc = HR.doc();
        for intent in HR_INTENTS {
            assert_eq!(doc.matches(&format!("- {}:", intent.name)).count(), 1);
        }
        assert!(HR_GUIDANCE.ends_with('\n'));
    }

    #[test]
    fn an_exclusion_is_a_sentence_not_a_shrug() {
        for excluded in HR_EXCLUDED {
            assert!(
                excluded.why.ends_with('.'),
                "{}: {}",
                excluded.route,
                excluded.why
            );
            assert!(
                !HR_INTENTS
                    .iter()
                    .any(|i| i.routes.contains(&excluded.route)),
                "{} is both a verb's route and excluded",
                excluded.route
            );
        }
    }

    #[test]
    fn nothing_here_offers_a_reason_a_kind_of_leave_or_a_note_to_read() {
        // The one mistake this module can make that nothing downstream
        // catches: a health fact composed by a model out of an absence. No
        // argument carries such a field, and the words forbid the inference.
        let doc = HR.doc();
        for forbidden in ["\"reason\"", "\"kind\"", "\"policy\"", "\"type\""] {
            assert!(!doc.contains(forbidden), "{forbidden}");
        }
        assert!(doc.contains("never state or guess WHY"));
        assert!(doc.contains("not illness"));
        assert!(HR_GUIDANCE.contains("never state, guess or imply WHY"));
        // A request's note is the app's, in both renderings the model reads.
        assert!(doc.contains("STAYS IN THE APP"));
        assert!(HR_GUIDANCE.contains("note stays in the app"));
    }

    #[test]
    fn silence_is_never_reported_as_presence() {
        let doc = HR.doc();
        assert!(doc.contains("Never answer with who is IN"));
        assert!(HR_GUIDANCE.contains("never what its silence implies"));
    }

    #[test]
    fn nothing_here_evaluates_ranks_or_scores_a_person() {
        // The AI Act posture of `docs/design/hr.md`, held in the words the
        // model reads and in the verbs that do not exist: screening is not a
        // scheduling cut, it is a refusal.
        assert!(HR_GUIDANCE.contains("Never rank, score, screen, shortlist or evaluate"));
        assert!(HR_GUIDANCE.contains("there is no verb for it"));
        assert!(HR_GUIDANCE.contains("no tally of how often somebody is off"));
        for forbidden in [
            "screen_cv",
            "rank_applicants",
            "score_candidate",
            "shortlist_applicants",
            "assess_performance",
            "attendance_score",
        ] {
            assert!(HR.find(forbidden).is_none(), "{forbidden}");
            assert!(!HR.doc().contains(forbidden), "{forbidden}");
        }
        // …and every hiring route is excluded rather than merely unmentioned.
        for route in [
            "/hr/openings",
            "/hr/applicants/{id}",
            "/hr/applicants/{id}/move",
        ] {
            assert!(HR.covers(route), "{route}");
        }
    }

    #[test]
    fn a_balance_is_the_askers_own_and_pay_is_never_readable() {
        let balance = HR
            .find("my_leave_balance")
            .expect("my_leave_balance is a verb");
        assert!(balance.purpose.contains("ASKER'S OWN"));
        assert!(balance.purpose.contains("NOBODY ELSE"));
        assert!(balance.args.is_empty(), "no argument names somebody else");
        assert!(HR_GUIDANCE.contains("A balance is the asker's own"));
        // The payroll export is refused by name, and no verb's wording offers
        // a private field.
        assert!(HR.covers("/hr/payroll-exports"));
        for refused in [
            "no pay",
            "no bank account",
            "no date of birth",
            "no home address",
            "no national id",
        ] {
            assert!(HR.doc().contains(refused), "{refused} is not refused");
        }
    }

    #[test]
    fn approving_leave_is_the_deciders_own_act_and_rejecting_stays_in_the_app() {
        let approve = HR
            .find("approve_leave_request")
            .expect("approve_leave_request is a verb");
        assert_eq!(approve.effect, Effect::Write);
        assert!(
            approve
                .purpose
                .contains("NEVER anybody for their own leave")
        );
        assert!(approve.purpose.contains("Rejecting stays in the app"));
        assert!(
            approve
                .preview
                .expect("a write has a preview")
                .contains("in your name")
        );
        assert!(
            !HR_INTENTS.iter().any(|i| i.name == "reject_leave_request"),
            "rejection is deliberately not a verb"
        );
        assert!(HR.covers("/hr/leave-requests/{id}/reject"));
    }

    #[test]
    fn the_letter_verb_can_only_fill_in_a_letter_the_company_wrote() {
        // The one mistake the letter verb can make that nothing downstream
        // catches: a model that, finding no template, writes the employment
        // confirmation itself. The executor answers such a proposal with a
        // 422 — these words are what stop the model getting that far.
        let letter = HR
            .find("draft_letter_from_template")
            .expect("draft_letter_from_template is a verb");
        assert!(letter.purpose.contains("THIS COMPANY'S OWN"));
        assert!(
            letter
                .purpose
                .contains("ONLY a letter the company has written")
        );
        assert!(
            letter
                .purpose
                .contains("NEVER write, extend, reword or invent a letter")
        );
        assert!(letter.purpose.contains("say which letters exist and stop"));
        assert!(letter.purpose.contains("sends nothing and tells nobody"));
        assert!(
            letter
                .preview
                .expect("a write has a preview")
                .contains("sent to nobody")
        );
    }
}
