//! Typed, atomic AI edit proposals for one alo Sites page.
//!
//! The model may describe only five operations. Every existing-section target
//! carries both an index and the section type expected at that index; a stale
//! or unclear target is refused as [`SiteEditError::Ambiguous`] instead of
//! silently editing a different section. Application is pure and atomic: a
//! cloned page is validated through the authoritative Sites schema before it
//! is returned, while the caller's original stays untouched.

use alo_store::{Section, SectionsEnvelope};
use serde::{Deserialize, Serialize};

use crate::ChatMessage;
use crate::agent::extract_json;

/// Current version of the Sites edit-operation envelope.
pub const SITE_EDIT_SCHEMA_VERSION: u64 = 1;
const MAX_EDIT_OPERATIONS: usize = 50;
const MAX_POINTER_CHARS: usize = 300;

/// An unambiguous reference to the section the model saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteSectionTarget {
    /// Zero-based index in the page envelope supplied to the model.
    pub index: usize,
    /// Expected wire type at that index (`hero`, `features`, ...).
    #[serde(rename = "type")]
    pub kind: String,
}

/// The closed edit vocabulary. Operations run in array order against a cloned
/// page; structural operations therefore affect the indices seen by later
/// operations, whose expected type protects against accidental retargeting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum SiteEditOperation {
    AddSection {
        at: usize,
        section: Section,
    },
    RemoveSection {
        target: SiteSectionTarget,
    },
    ReorderSection {
        target: SiteSectionTarget,
        to: usize,
    },
    SetProp {
        target: SiteSectionTarget,
        pointer: String,
        value: serde_json::Value,
    },
    RewriteCopy {
        target: SiteSectionTarget,
        pointer: String,
        text: String,
    },
}

/// One proposed atomic change set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteEditEnvelope {
    pub schema_version: u64,
    pub operations: Vec<SiteEditOperation>,
}

/// Why an edit proposal could not be parsed or safely applied.
#[derive(Debug, thiserror::Error)]
pub enum SiteEditError {
    #[error("site edit response did not contain one JSON object")]
    MissingObject,
    #[error(
        "unsupported site edit schema_version {0} (this build speaks {SITE_EDIT_SCHEMA_VERSION})"
    )]
    UnsupportedVersion(u64),
    #[error("site edit JSON does not match schema v{SITE_EDIT_SCHEMA_VERSION}: {0}")]
    Shape(#[from] serde_json::Error),
    #[error("site edit operation {operation} is ambiguous: {detail}")]
    Ambiguous { operation: usize, detail: String },
    #[error("site edit operation {operation} is invalid: {detail}")]
    Invalid { operation: usize, detail: String },
    #[error("site edit result is invalid: {0}")]
    InvalidResult(String),
}

const SITE_EDIT_SYSTEM: &str = r#"You propose precise edits to ONE alo Sites page. Reply with a SINGLE JSON object and nothing else: no prose, no markdown, no code fences.

Envelope: {"schema_version":1,"operations":[operation,...]}. Use 1-50 operations in the order they should run.
The only operations are:
- add_section: {"op":"add_section","at":number,"section":<one complete valid section object>}
- remove_section: {"op":"remove_section","target":{"index":number,"type":string}}
- reorder_section: {"op":"reorder_section","target":{"index":number,"type":string},"to":number}
- set_prop: {"op":"set_prop","target":{"index":number,"type":string},"pointer":string,"value":any JSON value}
- rewrite_copy: {"op":"rewrite_copy","target":{"index":number,"type":string},"pointer":string,"text":string}

Targets are zero-based and MUST repeat the exact section type at that index. JSON pointers follow RFC 6901, start with '/', and address a property inside the section; never change '/type'. Use set_prop for a non-copy value or to set/clear an optional property. Use rewrite_copy only for an existing string leaf. Structural operations change indices for later operations, so compute every later target against the result of earlier operations. Never invent facts, people, testimonials, prices, URLs, asset ids, or form ids. Make only the change requested. Output ONLY the JSON object."#;

/// Builds the edit conversation from the exact page the user is previewing.
/// The existing page is data in the user turn, never mixed into system rules.
pub fn site_edit_messages(
    page: &SectionsEnvelope,
    instruction: &str,
) -> Result<Vec<ChatMessage>, SiteEditError> {
    let page = serde_json::to_string(page)?;
    Ok(vec![
        ChatMessage {
            role: "system".to_owned(),
            content: SITE_EDIT_SYSTEM.to_owned(),
        },
        ChatMessage {
            role: "user".to_owned(),
            content: format!(
                "Current page sections:\n{page}\n\nRequested change:\n{}",
                instruction.trim()
            ),
        },
    ])
}

/// Strictly parses one edit proposal. Unknown operations and fields are
/// rejected before application.
pub fn parse_site_edit(text: &str) -> Result<SiteEditEnvelope, SiteEditError> {
    let json = extract_json(text).ok_or(SiteEditError::MissingObject)?;
    let value: serde_json::Value = serde_json::from_str(json)?;
    if !value.is_object() {
        return Err(SiteEditError::MissingObject);
    }
    if let Some(version) = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        && version != SITE_EDIT_SCHEMA_VERSION
    {
        return Err(SiteEditError::UnsupportedVersion(version));
    }
    let envelope: SiteEditEnvelope = serde_json::from_value(value)?;
    validate_envelope(&envelope)?;
    Ok(envelope)
}

/// Applies a proposal to a clone and returns the fully validated result.
/// Failure at any operation leaves `page` unchanged.
pub fn apply_site_edit(
    page: &SectionsEnvelope,
    edit: &SiteEditEnvelope,
) -> Result<SectionsEnvelope, SiteEditError> {
    validate_envelope(edit)?;
    let mut result = page.clone();

    for (operation, edit) in edit.operations.iter().enumerate() {
        match edit {
            SiteEditOperation::AddSection { at, section } => {
                if *at > result.sections.len() {
                    return Err(invalid(
                        operation,
                        format!("insertion index {at} is outside the page"),
                    ));
                }
                result.sections.insert(*at, section.clone());
            }
            SiteEditOperation::RemoveSection { target } => {
                target_index(&result.sections, target, operation)?;
                result.sections.remove(target.index);
            }
            SiteEditOperation::ReorderSection { target, to } => {
                target_index(&result.sections, target, operation)?;
                if *to >= result.sections.len() {
                    return Err(invalid(
                        operation,
                        format!("destination index {to} is outside the page"),
                    ));
                }
                let section = result.sections.remove(target.index);
                result.sections.insert(*to, section);
            }
            SiteEditOperation::SetProp {
                target,
                pointer,
                value,
            } => {
                let index = target_index(&result.sections, target, operation)?;
                let mut section = serde_json::to_value(&result.sections[index])?;
                set_pointer(&mut section, pointer, value.clone(), operation)?;
                result.sections[index] = serde_json::from_value(section)
                    .map_err(|error| invalid(operation, error.to_string()))?;
            }
            SiteEditOperation::RewriteCopy {
                target,
                pointer,
                text,
            } => {
                let index = target_index(&result.sections, target, operation)?;
                let mut section = serde_json::to_value(&result.sections[index])?;
                let value = section.pointer_mut(pointer).ok_or_else(|| {
                    ambiguous(operation, format!("{pointer} does not identify a property"))
                })?;
                if !value.is_string() {
                    return Err(invalid(
                        operation,
                        format!("{pointer} is not an existing text property"),
                    ));
                }
                *value = serde_json::Value::String(text.clone());
                result.sections[index] = serde_json::from_value(section)
                    .map_err(|error| invalid(operation, error.to_string()))?;
            }
        }
    }

    result
        .validate()
        .map_err(|error| SiteEditError::InvalidResult(error.to_string()))?;
    Ok(result)
}

fn validate_envelope(edit: &SiteEditEnvelope) -> Result<(), SiteEditError> {
    if edit.schema_version != SITE_EDIT_SCHEMA_VERSION {
        return Err(SiteEditError::UnsupportedVersion(edit.schema_version));
    }
    if edit.operations.is_empty() || edit.operations.len() > MAX_EDIT_OPERATIONS {
        return Err(invalid(
            0,
            format!("operations must contain 1-{MAX_EDIT_OPERATIONS} entries"),
        ));
    }
    for (operation, edit) in edit.operations.iter().enumerate() {
        match edit {
            SiteEditOperation::SetProp { pointer, .. }
            | SiteEditOperation::RewriteCopy { pointer, .. } => {
                validate_pointer(pointer, operation)?;
            }
            SiteEditOperation::AddSection { .. }
            | SiteEditOperation::RemoveSection { .. }
            | SiteEditOperation::ReorderSection { .. } => {}
        }
    }
    Ok(())
}

fn validate_pointer(pointer: &str, operation: usize) -> Result<(), SiteEditError> {
    if pointer.is_empty()
        || !pointer.starts_with('/')
        || pointer.chars().count() > MAX_POINTER_CHARS
    {
        return Err(invalid(
            operation,
            format!("pointer must be an RFC 6901 path of 1-{MAX_POINTER_CHARS} characters"),
        ));
    }
    if pointer == "/type" || pointer.starts_with("/type/") {
        return Err(invalid(operation, "the section type cannot be changed"));
    }
    for token in pointer.split('/').skip(1) {
        let mut chars = token.chars();
        while let Some(character) = chars.next() {
            if character == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return Err(invalid(operation, "pointer contains an invalid escape"));
            }
        }
    }
    Ok(())
}

fn target_index(
    sections: &[Section],
    target: &SiteSectionTarget,
    operation: usize,
) -> Result<usize, SiteEditError> {
    let section = sections.get(target.index).ok_or_else(|| {
        ambiguous(
            operation,
            format!("section index {} no longer exists", target.index),
        )
    })?;
    if section.kind() != target.kind {
        return Err(ambiguous(
            operation,
            format!(
                "section {} is `{}`, not expected `{}`",
                target.index,
                section.kind(),
                target.kind
            ),
        ));
    }
    Ok(target.index)
}

fn set_pointer(
    root: &mut serde_json::Value,
    pointer: &str,
    value: serde_json::Value,
    operation: usize,
) -> Result<(), SiteEditError> {
    let (parent, leaf) = pointer.rsplit_once('/').ok_or_else(|| {
        invalid(
            operation,
            "pointer must identify a property inside the section",
        )
    })?;
    let leaf = decode_pointer_token(leaf, operation)?;
    let parent = if parent.is_empty() {
        root
    } else {
        root.pointer_mut(parent)
            .ok_or_else(|| ambiguous(operation, format!("{parent} does not identify a value")))?
    };
    match parent {
        serde_json::Value::Object(object) => {
            object.insert(leaf, value);
            Ok(())
        }
        serde_json::Value::Array(array) => {
            let index = leaf
                .parse::<usize>()
                .map_err(|_| invalid(operation, "array pointer must end in an index"))?;
            let slot = array.get_mut(index).ok_or_else(|| {
                ambiguous(operation, format!("array index {index} does not exist"))
            })?;
            *slot = value;
            Ok(())
        }
        _ => Err(invalid(
            operation,
            "pointer parent is not an object or array",
        )),
    }
}

fn decode_pointer_token(token: &str, operation: usize) -> Result<String, SiteEditError> {
    let mut decoded = String::with_capacity(token.len());
    let mut chars = token.chars();
    while let Some(character) = chars.next() {
        if character != '~' {
            decoded.push(character);
            continue;
        }
        match chars.next() {
            Some('0') => decoded.push('~'),
            Some('1') => decoded.push('/'),
            _ => return Err(invalid(operation, "pointer contains an invalid escape")),
        }
    }
    Ok(decoded)
}

fn ambiguous(operation: usize, detail: impl Into<String>) -> SiteEditError {
    SiteEditError::Ambiguous {
        operation,
        detail: detail.into(),
    }
}

fn invalid(operation: usize, detail: impl Into<String>) -> SiteEditError {
    SiteEditError::Invalid {
        operation,
        detail: detail.into(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page() -> SectionsEnvelope {
        SectionsEnvelope::from_value(json!({
            "schema_version": 1,
            "sections": [
                {
                    "type": "hero",
                    "heading": "Old heading",
                    "subheading": null,
                    "image": null,
                    "primary_cta": null,
                    "secondary_cta": null
                },
                {
                    "type": "features",
                    "heading": "What we do",
                    "intro": null,
                    "items": [{"title": "Care", "body": "Thoughtful work", "icon": null}]
                },
                {"type": "footer", "text": "Acme", "links": []}
            ]
        }))
        .unwrap()
    }

    fn target(index: usize, kind: &str) -> SiteSectionTarget {
        SiteSectionTarget {
            index,
            kind: kind.to_owned(),
        }
    }

    fn edit(operations: Vec<SiteEditOperation>) -> SiteEditEnvelope {
        SiteEditEnvelope {
            schema_version: SITE_EDIT_SCHEMA_VERSION,
            operations,
        }
    }

    #[test]
    fn parser_is_closed_and_prompt_documents_all_five_operations() {
        let parsed = parse_site_edit(
            r#"{"schema_version":1,"operations":[{"op":"rewrite_copy","target":{"index":0,"type":"hero"},"pointer":"/heading","text":"A new heading"}]}"#,
        )
        .unwrap();
        assert_eq!(parsed.operations.len(), 1);

        for bad in [
            r#"{"schema_version":1,"extra":true,"operations":[]}"#,
            r#"{"schema_version":1,"operations":[{"op":"rotate_section","index":0}]}"#,
            r#"{"schema_version":2,"operations":[]}"#,
        ] {
            assert!(parse_site_edit(bad).is_err(), "must refuse {bad}");
        }

        let prompt = &site_edit_messages(&page(), "improve the hero").unwrap()[0].content;
        for operation in [
            "add_section",
            "remove_section",
            "reorder_section",
            "set_prop",
            "rewrite_copy",
        ] {
            assert!(prompt.contains(operation), "missing {operation}");
        }
    }

    #[test]
    fn add_remove_and_reorder_are_applied_in_order() {
        let result = apply_site_edit(
            &page(),
            &edit(vec![
                SiteEditOperation::AddSection {
                    at: 2,
                    section: serde_json::from_value(json!({
                        "type": "cta",
                        "heading": "Ready?",
                        "body": null,
                        "button": {"label": "Talk to us", "href": "/contact"}
                    }))
                    .unwrap(),
                },
                SiteEditOperation::ReorderSection {
                    target: target(2, "cta"),
                    to: 1,
                },
                SiteEditOperation::RemoveSection {
                    target: target(2, "features"),
                },
            ]),
        )
        .unwrap();

        assert_eq!(
            result
                .sections
                .iter()
                .map(Section::kind)
                .collect::<Vec<_>>(),
            ["hero", "cta", "footer"]
        );
    }

    #[test]
    fn set_prop_can_fill_an_absent_optional_and_update_a_nested_value() {
        let result = apply_site_edit(
            &page(),
            &edit(vec![
                SiteEditOperation::SetProp {
                    target: target(0, "hero"),
                    pointer: "/subheading".to_owned(),
                    value: json!("Clear, useful work"),
                },
                SiteEditOperation::SetProp {
                    target: target(1, "features"),
                    pointer: "/items/0/icon".to_owned(),
                    value: json!("heart"),
                },
            ]),
        )
        .unwrap();

        let value = result.to_value().unwrap();
        assert_eq!(
            value.pointer("/sections/0/subheading"),
            Some(&json!("Clear, useful work"))
        );
        assert_eq!(
            value.pointer("/sections/1/items/0/icon"),
            Some(&json!("heart"))
        );
    }

    #[test]
    fn rewrite_copy_changes_only_an_existing_string_leaf() {
        let result = apply_site_edit(
            &page(),
            &edit(vec![SiteEditOperation::RewriteCopy {
                target: target(0, "hero"),
                pointer: "/heading".to_owned(),
                text: "Work that earns trust".to_owned(),
            }]),
        )
        .unwrap();
        assert_eq!(
            result.to_value().unwrap().pointer("/sections/0/heading"),
            Some(&json!("Work that earns trust"))
        );

        let error = apply_site_edit(
            &page(),
            &edit(vec![SiteEditOperation::RewriteCopy {
                target: target(1, "features"),
                pointer: "/items".to_owned(),
                text: "not an array".to_owned(),
            }]),
        )
        .unwrap_err();
        assert!(matches!(error, SiteEditError::Invalid { .. }));
    }

    #[test]
    fn stale_or_unclear_targets_are_a_typed_ambiguity() {
        let original = page();
        let error = apply_site_edit(
            &original,
            &edit(vec![SiteEditOperation::RemoveSection {
                target: target(1, "hero"),
            }]),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SiteEditError::Ambiguous { operation: 0, .. }
        ));
        assert_eq!(original.sections[1].kind(), "features");

        let missing = apply_site_edit(
            &original,
            &edit(vec![SiteEditOperation::RewriteCopy {
                target: target(0, "hero"),
                pointer: "/caption".to_owned(),
                text: "Guess".to_owned(),
            }]),
        )
        .unwrap_err();
        assert!(matches!(missing, SiteEditError::Ambiguous { .. }));
    }

    #[test]
    fn application_is_atomic_and_the_result_must_pass_the_site_write_gate() {
        let original = page();
        let error = apply_site_edit(
            &original,
            &edit(vec![
                SiteEditOperation::RewriteCopy {
                    target: target(0, "hero"),
                    pointer: "/heading".to_owned(),
                    text: "This would be valid".to_owned(),
                },
                SiteEditOperation::SetProp {
                    target: target(2, "footer"),
                    pointer: "/links".to_owned(),
                    value: json!([{"label":"Unsafe","href":"javascript:alert(1)"}]),
                },
            ]),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SiteEditError::InvalidResult(_) | SiteEditError::Invalid { .. }
        ));
        assert_eq!(
            original.to_value().unwrap().pointer("/sections/0/heading"),
            Some(&json!("Old heading"))
        );
    }

    #[test]
    fn section_type_and_invalid_pointers_can_never_be_changed() {
        for pointer in ["/type", "type", "/bad~escape"] {
            let error = apply_site_edit(
                &page(),
                &edit(vec![SiteEditOperation::SetProp {
                    target: target(0, "hero"),
                    pointer: pointer.to_owned(),
                    value: json!("footer"),
                }]),
            )
            .unwrap_err();
            assert!(matches!(error, SiteEditError::Invalid { .. }));
        }
    }

    #[test]
    fn page_context_and_instruction_are_kept_in_the_user_turn() {
        let messages = site_edit_messages(&page(), "  Make the hero warmer.  ").unwrap();
        assert_eq!(messages.len(), 2);
        assert!(!messages[0].content.contains("Old heading"));
        assert!(messages[1].content.contains("Old heading"));
        assert!(messages[1].content.ends_with("Make the hero warmer."));
        assert!(
            messages[0]
                .content
                .ends_with("Output ONLY the JSON object.")
        );
    }

    #[test]
    fn output_keeps_the_sections_schema_version() {
        let result = apply_site_edit(
            &page(),
            &edit(vec![SiteEditOperation::RewriteCopy {
                target: target(2, "footer"),
                pointer: "/text".to_owned(),
                text: "Acme 2026".to_owned(),
            }]),
        )
        .unwrap();
        assert_eq!(result.schema_version, alo_store::SECTIONS_SCHEMA_VERSION);
    }
}
