//! Deserialized view of `contracts/rpc-client-options/v1/catalog.json`.
//!
//! The catalog is the single authority for the RPC client chaining surface.
//! Per-language method names are never authored: they are derived from
//! `option_id` so a language surface cannot drift from the contract.

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    pub catalog_version: String,
    pub rpc_path: String,
    pub description: String,
    pub exclusive_groups: Vec<ExclusiveGroup>,
    pub enums: Vec<CatalogEnum>,
    pub options: Vec<Option_>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExclusiveGroup {
    pub exclusive_group_id: String,
    pub summary: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default_option_id: Option<String>,
    #[serde(default)]
    pub implicit_plan_value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogEnum {
    pub enum_id: String,
    pub namespace: String,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnumVariant {
    pub variant_id: String,
    pub wire_value: serde_json::Value,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub content_encoding: Option<String>,
}

/// Which builder surfaces an option appears on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppliesTo {
    Unary,
    Stream,
    Both,
}

impl AppliesTo {
    pub fn on_unary(self) -> bool {
        matches!(self, Self::Unary | Self::Both)
    }

    pub fn on_stream(self) -> bool {
        matches!(self, Self::Stream | Self::Both)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unary => "unary",
            Self::Stream => "stream",
            Self::Both => "both",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arity {
    Once,
    Many,
}

impl Arity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Many => "many",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(non_camel_case_types)]
pub struct Option_ {
    pub option_id: String,
    pub group: String,
    pub applies_to: AppliesTo,
    pub arity: Arity,
    #[serde(default)]
    pub exclusive_group: Option<String>,
    pub terminal: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub local_only: bool,
    pub params: Vec<Param>,
    #[serde(default)]
    pub plan_field: Option<String>,
    #[serde(default)]
    pub plan_value: Option<serde_json::Value>,
    #[serde(default)]
    pub wire: Option<Wire>,
    #[serde(default)]
    pub returns: Option<String>,
    #[serde(default)]
    pub requires_capability: Option<String>,
    #[serde(default)]
    pub requires_options: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub enum_id: Option<String>,
    #[serde(default)]
    pub minimum: Option<f64>,
    #[serde(default)]
    pub maximum: Option<f64>,
    #[serde(default)]
    pub max_length: Option<u32>,
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Wire {
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub drop_headers: Vec<String>,
    #[serde(default)]
    pub headers_from_enum: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("catalog is not readable: {0}")]
    Read(#[from] std::io::Error),
    #[error("catalog is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("catalog integrity failure: {0}")]
    Integrity(String),
}

impl Catalog {
    pub fn parse(source: &str) -> Result<Self, CatalogError> {
        let catalog: Self = serde_json::from_str(source)?;
        catalog.check_integrity()?;
        Ok(catalog)
    }

    /// Fail closed on anything that would make generation ambiguous.
    fn check_integrity(&self) -> Result<(), CatalogError> {
        let fail = |message: String| Err(CatalogError::Integrity(message));

        // Options are stored sorted by option_id so the authored file is
        // diff-stable and generation order never depends on authoring order.
        let ids: Vec<&str> = self.options.iter().map(|o| o.option_id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        if ids != sorted {
            return fail("options must be stored sorted by option_id".to_owned());
        }
        sorted.dedup();
        if sorted.len() != ids.len() {
            return fail("duplicate option_id".to_owned());
        }

        let group_ids: Vec<&str> = self
            .exclusive_groups
            .iter()
            .map(|g| g.exclusive_group_id.as_str())
            .collect();
        let enum_ids: Vec<&str> = self.enums.iter().map(|e| e.enum_id.as_str()).collect();

        for option in &self.options {
            if !is_snake_case(&option.option_id) {
                return fail(format!("option_id is not snake_case: {}", option.option_id));
            }
            if let Some(group) = &option.exclusive_group {
                if !group_ids.contains(&group.as_str()) {
                    return fail(format!(
                        "{} references undeclared exclusive_group {group}",
                        option.option_id
                    ));
                }
                if option.arity != Arity::Once {
                    return fail(format!(
                        "{} is in an exclusive group but is not arity once",
                        option.option_id
                    ));
                }
            }
            if option.terminal && option.plan_field.is_some() {
                return fail(format!(
                    "terminal option {} must not write a plan field",
                    option.option_id
                ));
            }
            if option.secret && option.plan_value.is_none() {
                return fail(format!(
                    "secret option {} must record a non-credential plan value",
                    option.option_id
                ));
            }
            for param in &option.params {
                if !is_snake_case(&param.name) {
                    return fail(format!(
                        "{}: param name is not snake_case: {}",
                        option.option_id, param.name
                    ));
                }
                if param.ty == "enum" {
                    match &param.enum_id {
                        None => {
                            return fail(format!(
                                "{}: enum param {} has no enum_id",
                                option.option_id, param.name
                            ))
                        }
                        Some(id) if !enum_ids.contains(&id.as_str()) => {
                            return fail(format!(
                                "{}: param {} references undeclared enum {id}",
                                option.option_id, param.name
                            ))
                        }
                        Some(_) => {}
                    }
                }
            }
        }

        for group in &self.exclusive_groups {
            if let Some(default_id) = &group.default_option_id {
                if !ids.contains(&default_id.as_str()) {
                    return fail(format!(
                        "exclusive group {} defaults to unknown option {default_id}",
                        group.exclusive_group_id
                    ));
                }
            }
        }

        // Every exclusive group must actually constrain something, otherwise a
        // generated type-state transition would be dead weight in every client.
        for group_id in &group_ids {
            let members = self
                .options
                .iter()
                .filter(|o| o.exclusive_group.as_deref() == Some(*group_id))
                .count();
            if members < 2 {
                return fail(format!(
                    "exclusive group {group_id} has {members} members; it cannot express a contradiction"
                ));
            }
        }

        if !self.rpc_path.starts_with('/') {
            return fail("rpc_path must be an absolute path".to_owned());
        }
        Ok(())
    }

    pub fn enum_by_id(&self, enum_id: &str) -> Option<&CatalogEnum> {
        self.enums.iter().find(|e| e.enum_id == enum_id)
    }

    /// Options reachable on the unary builder, in canonical order.
    pub fn unary_options(&self) -> impl Iterator<Item = &Option_> {
        self.options.iter().filter(|o| o.applies_to.on_unary())
    }

    /// Options reachable on the streaming builder, in canonical order.
    pub fn stream_options(&self) -> impl Iterator<Item = &Option_> {
        self.options.iter().filter(|o| o.applies_to.on_stream())
    }

    /// Distinct option groups in first-appearance order over the sorted options,
    /// then sorted, so documentation section order is a pure function of content.
    pub fn groups(&self) -> Vec<&str> {
        let mut groups: Vec<&str> = self.options.iter().map(|o| o.group.as_str()).collect();
        groups.sort_unstable();
        groups.dedup();
        groups
    }
}

fn is_snake_case(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !value.starts_with('_')
        && !value.ends_with('_')
        && !value.contains("__")
}

// Fields added alongside the derived-schema and fixture emitters.
impl ExclusiveGroup {
    /// Plan value that stands for "no member of this group was selected".
    pub fn implicit_plan_value_ref(&self) -> Option<&serde_json::Value> {
        self.implicit_plan_value.as_ref()
    }
}

#[cfg(test)]
mod embedded_tests {
    use super::super::EMBEDDED_CATALOG;
    use super::*;

    #[test]
    fn the_embedded_catalog_matches_the_authored_file() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository root");
        let authored = std::fs::read_to_string(root.join(super::super::CATALOG_PATH))
            .expect("authored catalog is readable");
        assert_eq!(
            EMBEDDED_CATALOG, authored,
            "the embedded catalog drifted from the authored file"
        );
    }

    #[test]
    fn the_embedded_catalog_passes_its_own_integrity_checks() {
        let catalog = Catalog::embedded().expect("embedded catalog is well formed");
        assert!(catalog.options.len() >= 50);
        assert!(catalog.unary_options().any(|o| o.option_id == "make_call"));
        assert!(catalog.stream_options().any(|o| o.option_id == "stream"));
        assert!(!catalog.stream_options().any(|o| o.option_id == "make_call"));
        assert!(!catalog.unary_options().any(|o| o.option_id == "stream"));
    }
}
