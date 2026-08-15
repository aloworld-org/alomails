//! Executing alo Drive's agent tools (ADR 0034, queue item A2.5) — the acting
//! half of what [`alo_ai::agent_drive`] describes to the model.
//!
//! Three reads run inside the turn ([`crate::agent_turn`]); the two that change
//! the Drive run only from [`crate::agent::agent_execute`], after the person who
//! asked approved the proposal. Everything here goes through the caller's own
//! tenant-scoped store handle, so the Drive agent reaches exactly the files the
//! person who asked could already open — a file of another tenant's, or of a
//! colleague's private Drive, is not merely refused here, it is not among the
//! things that can be named ([`alo_store::AccountStore::drive_find`] and
//! `drive_list` are personal and tenant-scoped, and the resolver picks out of
//! what they return).
//!
//! Five rules shape this module, and none of them is thin glue:
//!
//! - **A summary is written from the file, or it is not written.** `file_read`
//!   hands over what the file actually says; a file whose bytes are not text is
//!   refused **by name and by what it is**, because a plausible summary of a PDF
//!   nobody read is the failure mode this tool exists to close.
//! - **Reading a file hands back no address a write could use.** Drive's read is
//!   running text: no block ids, no cell references. Editing a document is
//!   `alo Docs`' job ([`crate::agent_docs`]) and editing a spreadsheet is
//!   `alo Sheets`' ([`crate::agent_sheets`]); this module can rename a file and
//!   move it, and cannot touch a byte inside one.
//! - **A rename cannot make a file stop opening.** The extension survives
//!   whatever was proposed, and a name a sibling already has is refused rather
//!   than made unique — two files called the same thing in one folder is a
//!   person's decision, not a server's.
//! - **A move stays inside the person's own Drive.**
//!   [`alo_store::AccountStore::drive_move`] re-scopes a node's access (ADR
//!   0027), so a move into a Space would hand the file to everybody in it. The
//!   destination here is always [`DriveLocation::Personal`], and the folder is
//!   found by walking the caller's own tree.
//! - **Every write is checked against the Drive before it is carried out**, and
//!   a write that would change nothing writes nothing: renaming a file to what
//!   it is already called, or moving it into the folder it is already in, comes
//!   back with a reason code and no `updated_at` bumped.

use axum::Json;
use serde_json::{Value, json};

use alo_store::{BlobId, DriveLocation, DriveNode, DriveNodeId};

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::state::Account;

/// The largest file this reads. Past this the decode costs more than the turn
/// is worth, and the honest answer is to say so rather than to spend a minute
/// on it — the same bound [`crate::agent_docs`] puts on a document.
const MAX_FILE_BYTES: i64 = 8 * 1024 * 1024;

/// How much text one `file_read` hands back at most, and its default.
pub(crate) const MAX_TEXT_CHARS: usize = 20_000;
pub(crate) const DEFAULT_TEXT_CHARS: usize = 6_000;

/// How far into the caller's own folder tree a destination is looked for, and
/// how many folders are considered at all. A Drive is a tree of unknown size and
/// a tool call is not the place to walk all of it; a folder deeper than this is
/// reported as not found, with the folders that were seen listed.
const MAX_FOLDER_DEPTH: u32 = 6;
const MAX_FOLDERS: usize = 300;

/// The longest name a rename may propose. Longer than any filesystem accepts,
/// and long enough that no real title trips on it.
const MAX_NAME_CHARS: usize = 200;

// ---- the reading tools -------------------------------------------------------

/// `find_file` — files in the caller's Drive matching what they called it.
///
/// # Errors
/// 400 when no query was given; 500 on a store failure.
pub async fn execute_find_file(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    // The model was told to ask which file rather than search for something
    // plausible; if it did neither, say so plainly instead of returning the
    // first twenty files in the drive.
    let query = string_arg(args, "query").ok_or_else(|| unprocessable("query is required"))?;
    let query = query.trim();
    if query.is_empty() {
        return Err(unprocessable("query is required"));
    }
    let limit = args.get("limit").and_then(Value::as_i64).unwrap_or(10);
    let found = account
        .acc
        .drive_find(query, limit)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "driveFiles",
            "query": query,
            "files": found.iter().map(file_ref).collect::<Vec<_>>(),
        }
    })))
}

/// `file_read` — what a file in the caller's Drive says, as running text.
///
/// A document is flattened out of the same block array the editor stores; a
/// plain-text file is decoded from its bytes. Everything else is refused by name
/// and by what it is, and the refusal says which agent *can* open it where one
/// can — an answer the model can pass on is worth more than a failure it has to
/// paraphrase.
///
/// # Errors
/// `422` when no file of the caller's matches the name, when the file is too
/// large, or when its contents are not text; the store's own failure otherwise.
pub async fn execute_file_read(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let node = resolve_file(account, args, "file").await?;
    let wanted = args
        .get("chars")
        .and_then(Value::as_u64)
        .and_then(|chars| usize::try_from(chars).ok())
        .unwrap_or(DEFAULT_TEXT_CHARS)
        .clamp(1, MAX_TEXT_CHARS);

    let text = match node.kind.as_str() {
        "doc" => document_text(account, &node).await?,
        // A spreadsheet and a slide deck are structures rather than prose, and
        // each has (or will have) an agent that understands its own addresses.
        // Handing back a JSON blob as "the text of the file" would be an answer
        // in the shape of a summary and wrong in every detail.
        "sheet" => {
            return Err(unprocessable(format!(
                "{} is a spreadsheet — ask the alo Sheets agent to read it, so the answer names the cells it came from",
                node.name
            )));
        }
        "folder" => {
            return Err(unprocessable(format!(
                "{} is a folder, not a file",
                node.name
            )));
        }
        _ => blob_text(account, &node).await?,
    };

    let truncated = text.chars().count() > wanted;
    let shown: String = text.chars().take(wanted).collect();
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "driveFileText",
            "file": file_ref(&node),
            "text": shown,
            // Said plainly: what came back is a window, and there is more.
            "truncated": truncated,
            "words": text.split_whitespace().count(),
        }
    })))
}

// ---- the writing tools -------------------------------------------------------

/// `file_rename` — a new name for a file in the caller's Drive.
///
/// Three refusals do the work, and all three happen **before** anything is
/// written: a file that is not the caller's to write, a name a sibling already
/// has, and a name that is not a name at all. The extension survives whatever
/// was proposed — a rename that turns `report.pdf` into `report` is a file that
/// stops opening, and an approved proposal must not be able to do that.
///
/// # Errors
/// `422` when the file cannot be named, when the new name is missing, empty,
/// too long, carries a path separator, or is already a sibling's; the store's
/// own failure otherwise.
pub async fn execute_file_rename(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let node = resolve_file(account, args, "file").await?;
    require_writable(account, &node).await?;
    let wanted = string_arg(args, "name")
        .ok_or_else(|| unprocessable("say what the file should be called"))?;
    let name = keeping_extension(&node.name, &checked_name(&wanted)?);

    // Renaming a file to what it is already called is not a failure and not a
    // change: no version, no `updated_at`, and a reason the model can say out
    // loud.
    if name == node.name {
        return Ok(Json(json!({
            "ok": true,
            "result": {
                "kind": "driveFileRenamed",
                "file": file_ref(&node),
                "was": node.name,
                "now": name,
                "changed": false,
                "reason": "nameUnchanged",
            }
        })));
    }
    if let Some(clash) = sibling_named(account, &node, &name).await? {
        return Err(unprocessable(format!(
            "{} is already in that folder, so {} cannot be called that too",
            clash.name, node.name
        )));
    }

    account
        .acc
        .drive_rename(&node.id, &name)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "driveFileRenamed",
            "file": file_ref(&node),
            "was": node.name,
            "now": name,
            "changed": true,
            "reason": Value::Null,
        }
    })))
}

/// `file_move` — a file into another folder of the caller's own Drive.
///
/// The destination is always [`DriveLocation::Personal`]. That is the whole of
/// "a move never changes who can read the file": `drive_move` re-scopes a node's
/// access (ADR 0027), so a destination the caller could name in a Space would
/// hand the file to everybody in that Space — a decision nobody delegates to an
/// agent, and one this tool therefore cannot express.
///
/// # Errors
/// `422` when the file cannot be moved, when no folder of the caller's has that
/// name, or when a file of the same name is already there; the store's own
/// failure otherwise.
pub async fn execute_file_move(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let node = resolve_file(account, args, "file").await?;
    require_writable(account, &node).await?;
    let folders = own_folders(account).await?;
    let dest: Option<DriveNode> = match string_arg(args, "folder") {
        None => None,
        Some(wanted) => {
            if folders.is_empty() {
                return Err(unprocessable(
                    "there is no folder in your drive yet — a file can only move to the top level",
                ));
            }
            Some(
                pick(
                    &wanted,
                    folders
                        .iter()
                        .map(|folder| (folder.name.as_str(), folder.clone()))
                        .collect(),
                    "folder",
                )
                .map_err(|problem| folder_refusal(problem, &folders))?,
            )
        }
    };
    let dest_id = dest.as_ref().map(|folder| folder.id.clone());

    // Already there: a reason code rather than a move that does nothing but
    // bump a timestamp and make a version history lie.
    if node.parent_id.as_ref().map(DriveNodeId::as_str) == dest_id.as_ref().map(DriveNodeId::as_str)
    {
        return Ok(Json(json!({
            "ok": true,
            "result": {
                "kind": "driveFileMoved",
                "file": file_ref(&node),
                "folder": dest.as_ref().map(folder_ref),
                "changed": false,
                "reason": "alreadyThere",
            }
        })));
    }
    // Something of that name is already in the destination. Usually reached
    // through a *folder* sharing the file's name: two **files** of one name make
    // the name ambiguous, so `resolve_file` refuses before this line is read.
    // Kept because "the destination decides what may land in it" is the rule,
    // and it should not depend on how the source happened to be resolved.
    if let Some(clash) = named_in(account, dest_id.as_ref(), &node.name).await? {
        return Err(unprocessable(format!(
            "there is already a {} in {}",
            clash.name,
            dest.as_ref()
                .map_or("the top level of your drive", |f| f.name.as_str())
        )));
    }

    account
        .acc
        .drive_move(&node.id, &DriveLocation::Personal, dest_id.as_ref())
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "driveFileMoved",
            "file": file_ref(&node),
            "folder": dest.as_ref().map(folder_ref),
            "changed": true,
            "reason": Value::Null,
        }
    })))
}

// ---- resolving ---------------------------------------------------------------

/// The file an argument names, out of the caller's own Drive.
///
/// Names, never ids — [`alo_ai::agent_drive`]'s first rule. The candidates come
/// from `drive_find`, which is personal, non-trashed and tenant-scoped, so a
/// file belonging to another tenant or to a colleague is not among the things
/// that can be named here.
async fn resolve_file(account: &Account, args: &Value, key: &str) -> Result<DriveNode, Problem> {
    let wanted = string_arg(args, key)
        .ok_or_else(|| unprocessable("say which file, by the name it has in your drive"))?;
    let found = account
        .acc
        .drive_find(&wanted, 20)
        .await
        .map_err(map_store_err)?;
    if found.is_empty() {
        return Err(unprocessable(format!(
            "no file of yours is called {wanted}"
        )));
    }
    pick(
        &wanted,
        found
            .iter()
            .map(|node| (node.name.as_str(), node.clone()))
            .collect(),
        "file",
    )
}

/// Refuses a write on a file the caller can see but may not change, naming it.
async fn require_writable(account: &Account, node: &DriveNode) -> Result<(), Problem> {
    let writable = account
        .acc
        .drive_writable(&node.id)
        .await
        .map_err(map_store_err)?;
    if writable {
        Ok(())
    } else {
        Err(unprocessable(format!(
            "{} is not yours to change",
            node.name
        )))
    }
}

/// Every folder of the caller's own Drive, breadth first and bounded.
///
/// `drive_find` deliberately excludes folders (somebody asking where a file is
/// does not mean the folder), so a destination is found by walking the tree the
/// caller can already see. Bounded twice — by depth and by count — because a
/// tool call is not the place to enumerate an arbitrarily large Drive; a folder
/// past the bound reads as not found, which the refusal states by listing what
/// *was* seen.
async fn own_folders(account: &Account) -> Result<Vec<DriveNode>, Problem> {
    let mut folders: Vec<DriveNode> = Vec::new();
    let mut frontier: Vec<Option<DriveNodeId>> = vec![None];
    for _ in 0..MAX_FOLDER_DEPTH {
        if frontier.is_empty() || folders.len() >= MAX_FOLDERS {
            break;
        }
        let mut next: Vec<Option<DriveNodeId>> = Vec::new();
        for parent in &frontier {
            let children = account
                .acc
                .drive_list(&DriveLocation::Personal, parent.as_ref())
                .await
                .map_err(map_store_err)?;
            for child in children.into_iter().filter(|node| node.kind == "folder") {
                if folders.len() >= MAX_FOLDERS {
                    break;
                }
                next.push(Some(child.id.clone()));
                folders.push(child);
            }
        }
        frontier = next;
    }
    Ok(folders)
}

/// A refusal naming a folder, rewritten to list the folders there are.
///
/// `pick`'s own words are right about the ambiguous case and thin about the
/// absent one: "no folder of yours is called Invoices" leaves the user guessing
/// what the folders are actually called, which is the recognition-over-recall
/// law (`docs/design/ux-principles.md`) read backwards.
fn folder_refusal(problem: Problem, folders: &[DriveNode]) -> Problem {
    match problem.detail.as_deref() {
        Some(detail) if detail.starts_with("no folder") => unprocessable(format!(
            "{detail} — you have: {}",
            folders
                .iter()
                .map(|folder| folder.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => problem,
    }
}

/// The sibling of `node` already called `name`, if there is one.
async fn sibling_named(
    account: &Account,
    node: &DriveNode,
    name: &str,
) -> Result<Option<DriveNode>, Problem> {
    let found = named_in(account, node.parent_id.as_ref(), name).await?;
    Ok(found.filter(|other| other.id.as_str() != node.id.as_str()))
}

/// The node called `name` directly inside `parent` (the Drive's top level when
/// `None`), if there is one.
async fn named_in(
    account: &Account,
    parent: Option<&DriveNodeId>,
    name: &str,
) -> Result<Option<DriveNode>, Problem> {
    let siblings = account
        .acc
        .drive_list(&DriveLocation::Personal, parent)
        .await
        .map_err(map_store_err)?;
    Ok(siblings
        .into_iter()
        .find(|other| other.name.eq_ignore_ascii_case(name)))
}

// ---- names -------------------------------------------------------------------

/// A proposed name, checked for being a name at all.
///
/// A path separator is the one character a filename must not carry: a rename to
/// `../secrets/report` would read as a name in this store and as a traversal in
/// anything that later writes the tree to a filesystem, and the cheapest place
/// to refuse that is the only place a name is set.
fn checked_name(wanted: &str) -> Result<String, Problem> {
    let name = wanted.trim();
    if name.is_empty() {
        return Err(unprocessable("say what the file should be called"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(unprocessable(format!(
            "a file's name is at most {MAX_NAME_CHARS} characters"
        )));
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(unprocessable(
            "a file's name cannot contain a path, only the name itself",
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(unprocessable(
            "a file's name cannot contain control characters",
        ));
    }
    Ok(name.to_owned())
}

/// The proposed name, carrying the file's own extension whether or not the model
/// remembered it.
///
/// A file that stops opening is the damage an approved rename could otherwise
/// do, and no wording of a description prevents a model from proposing
/// `Q3 report` for `q3.xlsx`. So the extension is not the model's to change:
/// it is appended when the proposal does not already end in it.
fn keeping_extension(current: &str, proposed: &str) -> String {
    let Some(extension) = extension_of(current) else {
        return proposed.to_owned();
    };
    if extension_of(proposed).is_some_and(|proposed| proposed.eq_ignore_ascii_case(&extension)) {
        return proposed.to_owned();
    }
    format!("{proposed}.{extension}")
}

/// A filename's extension, lowercased — `None` for a name with no dot, a name
/// that only starts with one (`.env` is a name, not an extension), or an
/// "extension" too long to be one.
fn extension_of(name: &str) -> Option<String> {
    let (stem, extension) = name.rsplit_once('.')?;
    if stem.is_empty() || extension.is_empty() || extension.chars().count() > 12 {
        return None;
    }
    if !extension.chars().all(char::is_alphanumeric) {
        return None;
    }
    Some(extension.to_lowercase())
}

// ---- reading bytes -------------------------------------------------------------

/// A document node's blocks, flattened into the prose a summary is written from.
///
/// The same block array [`crate::agent_docs`] reads, without the ids: a heading
/// keeps its level as a marker so the shape of the document survives, and a list
/// item keeps its bullet. Nothing here is an address, which is the difference
/// between Drive reading a file and Docs reading a document.
async fn document_text(account: &Account, node: &DriveNode) -> Result<String, Problem> {
    let raw = blob_bytes(account, node).await?;
    if raw.is_empty() {
        return Ok(String::new());
    }
    let value: Value = serde_json::from_slice(&raw)
        .map_err(|_| unprocessable(format!("{} cannot be read as a document", node.name)))?;
    let document = alo_ai::doc_blocks::Document::read(&value)
        .map_err(|_| unprocessable(format!("{} cannot be read as a document", node.name)))?;
    let mut out = String::new();
    for block in &document.blocks {
        if block.text.trim().is_empty() {
            continue;
        }
        if block.is_heading() {
            let level = usize::try_from(block.level.unwrap_or(1))
                .unwrap_or(1)
                .clamp(1, 6);
            out.push_str(&format!("\n{} {}\n", "#".repeat(level), block.text));
        } else if block.kind.ends_with("ListItem") {
            out.push_str(&format!("- {}\n", block.text));
        } else {
            out.push_str(&block.text);
            out.push('\n');
        }
    }
    Ok(out.trim().to_owned())
}

/// An uploaded file's bytes, decoded — or the refusal that names what it is.
async fn blob_text(account: &Account, node: &DriveNode) -> Result<String, Problem> {
    let raw = blob_bytes(account, node).await?;
    if raw.is_empty() {
        return Ok(String::new());
    }
    if !looks_textual(&node.name, node.content_type.as_deref()) {
        return Err(not_text(node));
    }
    // Textual by its type and still not valid UTF-8: a mislabelled binary, or a
    // legacy encoding we would be guessing at. Refused the same way, because a
    // lossy decode reads like text and summarises like nonsense.
    String::from_utf8(raw).map_err(|_| not_text(node))
}

/// The `422` a file whose contents are not text earns, naming the file and what
/// it is rather than saying "unsupported".
pub(crate) fn not_text_named(name: &str, what: &str) -> Problem {
    unprocessable(format!(
        "{name} is {what}, and its text cannot be read here — say so rather than describing what it might contain"
    ))
}

fn not_text(node: &DriveNode) -> Problem {
    not_text_named(&node.name, &describe(node))
}

/// What a file is, in the words a refusal uses: its extension when it has a
/// telling one, else its media type, else the bare fact.
fn describe(node: &DriveNode) -> String {
    if let Some(extension) = extension_of(&node.name) {
        return format!("a .{extension} file");
    }
    match node.content_type.as_deref() {
        Some(ctype) if !ctype.is_empty() => format!("a {ctype} file"),
        _ => "not a text file".to_owned(),
    }
}

/// A node's stored bytes, bounded.
async fn blob_bytes(account: &Account, node: &DriveNode) -> Result<Vec<u8>, Problem> {
    let Some(blob) = node.blob_id.clone() else {
        // A node with no blob has never been saved: it is empty, and that is an
        // answer rather than a failure.
        return Ok(Vec::new());
    };
    if node.size > MAX_FILE_BYTES {
        return Err(unprocessable(format!(
            "{} is too large for the agent to read",
            node.name
        )));
    }
    let bytes = account
        .acc
        .blob_bytes_for_send(&BlobId::new(blob))
        .await
        .map_err(map_store_err)?;
    Ok(bytes.to_vec())
}

/// Whether a node's contents are text we can decode without guessing.
///
/// Type first, name second. A stored `content_type` is what the uploader said;
/// an extension is what the user sees. Either being textual is enough, because
/// a `.md` served as `application/octet-stream` is still a person's notes, and
/// the UTF-8 check downstream is what actually keeps a mislabelled binary out.
pub(crate) fn looks_textual(name: &str, content_type: Option<&str>) -> bool {
    let ctype = content_type
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    textual_type(&ctype)
        || extension_of(name).is_some_and(|extension| TEXT_EXTENSIONS.contains(&&*extension))
}

/// Media types whose payload is text. `text/*` plus the structured types that
/// are text in everything but their top-level type.
pub(crate) fn textual_type(ctype: &str) -> bool {
    ctype.starts_with("text/")
        || matches!(
            ctype,
            "application/json"
                | "application/xml"
                | "application/yaml"
                | "application/x-yaml"
                | "application/csv"
                | "application/x-ndjson"
        )
}

/// Extensions we will decode when the media type says nothing useful.
///
/// Deliberately short and deliberately not `.doc`/`.xlsx`/`.pdf`: an office
/// document is a zip and a PDF is a container, and there is no extractor for
/// either in this repo. The day one lands is the day a name goes here.
pub(crate) const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "csv", "tsv", "log", "json", "yaml", "yml", "xml", "html", "htm",
    "ics", "vcf", "srt", "vtt", "sql", "toml", "ini", "conf", "rst",
];

// ---- the shapes a result carries -------------------------------------------------

/// Which file a result is about.
fn file_ref(node: &DriveNode) -> Value {
    json!({
        "id": node.id.as_str(),
        "name": node.name,
        "kind": node.kind,
        "size": node.size,
        "contentType": node.content_type,
        "updatedAt": node.updated_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    })
}

/// Which folder a move landed in.
fn folder_ref(node: &DriveNode) -> Value {
    json!({ "id": node.id.as_str(), "name": node.name })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The rule that keeps an approved rename from producing a file nobody can
    /// open: the extension is the file's, not the model's.
    #[test]
    fn a_rename_carries_the_files_own_extension_across() {
        assert_eq!(keeping_extension("q3.xlsx", "Q3 report"), "Q3 report.xlsx");
        // Already right, and not doubled.
        assert_eq!(keeping_extension("q3.xlsx", "Q3.xlsx"), "Q3.xlsx");
        // Case is the user's business, not a reason to append.
        assert_eq!(keeping_extension("q3.XLSX", "Q3.xlsx"), "Q3.xlsx");
        // A proposal that changes the extension is corrected rather than
        // obeyed — `report.pdf` renamed to `report.txt` is still a PDF.
        assert_eq!(keeping_extension("r.pdf", "report.txt"), "report.txt.pdf");
        // A file with no extension gains none.
        assert_eq!(keeping_extension("README", "Readme notes"), "Readme notes");
        // A dotfile's leading dot is a name, not an extension.
        assert_eq!(keeping_extension(".env", "settings"), "settings");
    }

    #[test]
    fn an_extension_is_a_short_alphanumeric_tail_after_a_stem() {
        assert_eq!(extension_of("notes.md").as_deref(), Some("md"));
        assert_eq!(extension_of("a.b.TXT").as_deref(), Some("txt"));
        assert_eq!(extension_of("no-dot"), None);
        assert_eq!(extension_of(".env"), None);
        assert_eq!(extension_of("trailing."), None);
        assert_eq!(extension_of("x.averylongextension"), None);
        assert_eq!(extension_of("x.tar gz"), None);
    }

    /// A filename is a name. Anything that reads as a path is refused at the
    /// only place a name is set.
    #[test]
    fn a_proposed_name_cannot_be_a_path_or_a_blank() {
        assert_eq!(checked_name("  Report  ").unwrap(), "Report");
        for bad in ["", "   ", "../secrets", "a/b", "a\\b", ".", ".."] {
            assert!(checked_name(bad).is_err(), "{bad:?} must be refused");
        }
        assert!(checked_name("line\nbreak").is_err());
        assert!(checked_name(&"x".repeat(MAX_NAME_CHARS + 1)).is_err());
        assert!(checked_name(&"x".repeat(MAX_NAME_CHARS)).is_ok());
    }

    #[test]
    fn text_is_recognised_by_type_or_by_extension_and_office_files_are_neither() {
        assert!(textual_type("text/plain"));
        assert!(textual_type("text/csv"));
        assert!(textual_type("application/json"));
        assert!(!textual_type("application/pdf"));
        assert!(!textual_type(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        ));
        for office in ["pdf", "docx", "xlsx", "pptx", "zip", "png"] {
            assert!(
                !TEXT_EXTENSIONS.contains(&office),
                "{office} has no extractor in this repo"
            );
        }
    }
}
