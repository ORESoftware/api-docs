//! Proof-carrying receipts for deterministic API/MCP documentation builds.
//!
//! A receipt binds a rendered publication bundle to explicit source, generator,
//! configuration, and toolchain identities. It is evidence about a build; it
//! does not become an API contract authority and does not promote consumer-owned
//! documentation into platform-publisher documentation.

#![allow(clippy::needless_return)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::project::sha256_hex;
use crate::publication::DocsPublicationBundle;

pub const DOCS_BUILD_RECEIPT_SCHEMA_VERSION: &str = "ores.api-docs.build-receipt.v1";
pub const DOCS_BUILD_RECEIPT_GENERATOR: &str = "ores-api-docs";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocsBuildProvenance {
    pub source_repository: String,
    pub source_ref: String,
    pub source_sha: String,
    pub typespec_sha256: String,
    pub json_schema_sha256: String,
    pub config_sha256: String,
    pub generator_commit: String,
    pub toolchain_identity: String,
}

impl DocsBuildProvenance {
    pub fn validate(&self) -> Result<(), DocsBuildReceiptError> {
        if !valid_repository(&self.source_repository) {
            return Err(DocsBuildReceiptError::InvalidField("source_repository"));
        }
        if !valid_text(&self.source_ref, 256) {
            return Err(DocsBuildReceiptError::InvalidField("source_ref"));
        }
        if !valid_git_sha(&self.source_sha) {
            return Err(DocsBuildReceiptError::InvalidField("source_sha"));
        }
        for (name, digest) in [
            ("typespec_sha256", &self.typespec_sha256),
            ("json_schema_sha256", &self.json_schema_sha256),
            ("config_sha256", &self.config_sha256),
        ] {
            if !valid_sha256(digest) {
                return Err(DocsBuildReceiptError::InvalidField(name));
            }
        }
        if !valid_git_sha(&self.generator_commit) {
            return Err(DocsBuildReceiptError::InvalidField("generator_commit"));
        }
        if !valid_text(&self.toolchain_identity, 256) {
            return Err(DocsBuildReceiptError::InvalidField("toolchain_identity"));
        }

        return Ok(());
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocsBuildReceipt {
    pub schema_version: String,
    pub generator: String,
    pub service: String,
    pub publication_mode: String,
    pub producer: String,
    pub authority_scope: String,
    pub contract_sha256: String,
    pub provenance: DocsBuildProvenance,
    pub artifact_sha256: BTreeMap<String, String>,
    pub receipt_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct UnsignedDocsBuildReceipt<'a> {
    schema_version: &'a str,
    generator: &'a str,
    service: &'a str,
    publication_mode: &'a str,
    producer: &'a str,
    authority_scope: &'a str,
    contract_sha256: &'a str,
    provenance: &'a DocsBuildProvenance,
    artifact_sha256: &'a BTreeMap<String, String>,
}

#[derive(Debug, Error)]
pub enum DocsBuildReceiptError {
    #[error("invalid docs build receipt field: {0}")]
    InvalidField(&'static str),
    #[error("serialize docs build receipt: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("receipt metadata does not match the publication bundle: {0}")]
    PublicationMismatch(&'static str),
    #[error("artifact set or digest does not match the publication bundle: {0}")]
    ArtifactMismatch(String),
    #[error("receipt_sha256 does not match the canonical receipt payload")]
    ReceiptDigestMismatch,
}

impl DocsBuildReceipt {
    pub fn from_publication(
        bundle: &DocsPublicationBundle,
        provenance: DocsBuildProvenance,
    ) -> Result<Self, DocsBuildReceiptError> {
        provenance.validate()?;
        let artifact_sha256 = artifact_digests(bundle);
        let mut receipt = Self {
            schema_version: DOCS_BUILD_RECEIPT_SCHEMA_VERSION.to_owned(),
            generator: DOCS_BUILD_RECEIPT_GENERATOR.to_owned(),
            service: bundle.manifest.service.clone(),
            publication_mode: bundle.manifest.publication_mode.as_str().to_owned(),
            producer: bundle.manifest.producer.clone(),
            authority_scope: bundle.manifest.authority_scope.to_owned(),
            contract_sha256: bundle.manifest.contract_sha256.clone(),
            provenance,
            artifact_sha256,
            receipt_sha256: String::new(),
        };
        receipt.receipt_sha256 = receipt.compute_receipt_sha256()?;

        return Ok(receipt);
    }

    pub fn from_json_str(input: &str) -> Result<Self, DocsBuildReceiptError> {
        let receipt: Self = serde_json::from_str(input)?;
        receipt.validate_shape()?;
        if receipt.compute_receipt_sha256()? != receipt.receipt_sha256 {
            return Err(DocsBuildReceiptError::ReceiptDigestMismatch);
        }

        return Ok(receipt);
    }

    pub fn to_pretty_json(&self) -> Result<String, DocsBuildReceiptError> {
        self.validate_shape()?;
        let mut output = serde_json::to_string_pretty(self)?;
        output.push('\n');

        return Ok(output);
    }

    pub fn verify_publication(
        &self,
        bundle: &DocsPublicationBundle,
    ) -> Result<(), DocsBuildReceiptError> {
        self.validate_shape()?;
        if self.compute_receipt_sha256()? != self.receipt_sha256 {
            return Err(DocsBuildReceiptError::ReceiptDigestMismatch);
        }
        if self.service != bundle.manifest.service {
            return Err(DocsBuildReceiptError::PublicationMismatch("service"));
        }
        if self.publication_mode != bundle.manifest.publication_mode.as_str() {
            return Err(DocsBuildReceiptError::PublicationMismatch("publication_mode"));
        }
        if self.producer != bundle.manifest.producer {
            return Err(DocsBuildReceiptError::PublicationMismatch("producer"));
        }
        if self.authority_scope != bundle.manifest.authority_scope {
            return Err(DocsBuildReceiptError::PublicationMismatch("authority_scope"));
        }
        if self.contract_sha256 != bundle.manifest.contract_sha256 {
            return Err(DocsBuildReceiptError::PublicationMismatch("contract_sha256"));
        }

        let observed = artifact_digests(bundle);
        if observed != self.artifact_sha256 {
            let path = first_artifact_difference(&self.artifact_sha256, &observed);
            return Err(DocsBuildReceiptError::ArtifactMismatch(path));
        }

        return Ok(());
    }

    fn validate_shape(&self) -> Result<(), DocsBuildReceiptError> {
        if self.schema_version != DOCS_BUILD_RECEIPT_SCHEMA_VERSION {
            return Err(DocsBuildReceiptError::InvalidField("schema_version"));
        }
        if self.generator != DOCS_BUILD_RECEIPT_GENERATOR {
            return Err(DocsBuildReceiptError::InvalidField("generator"));
        }
        if !valid_text(&self.service, 128) {
            return Err(DocsBuildReceiptError::InvalidField("service"));
        }
        if !matches!(
            self.publication_mode.as_str(),
            "publisher_external" | "consumer_project"
        ) {
            return Err(DocsBuildReceiptError::InvalidField("publication_mode"));
        }
        if !valid_text(&self.producer, 128) {
            return Err(DocsBuildReceiptError::InvalidField("producer"));
        }
        if !matches!(
            self.authority_scope.as_str(),
            "platform_external_developers" | "project_owned"
        ) {
            return Err(DocsBuildReceiptError::InvalidField("authority_scope"));
        }
        if !valid_sha256(&self.contract_sha256) {
            return Err(DocsBuildReceiptError::InvalidField("contract_sha256"));
        }
        self.provenance.validate()?;
        if self.artifact_sha256.is_empty() {
            return Err(DocsBuildReceiptError::InvalidField("artifact_sha256"));
        }
        for (path, digest) in &self.artifact_sha256 {
            if !valid_artifact_path(path) || !valid_sha256(digest) {
                return Err(DocsBuildReceiptError::InvalidField("artifact_sha256"));
            }
        }
        if !valid_sha256(&self.receipt_sha256) {
            return Err(DocsBuildReceiptError::InvalidField("receipt_sha256"));
        }

        return Ok(());
    }

    fn compute_receipt_sha256(&self) -> Result<String, DocsBuildReceiptError> {
        let unsigned = UnsignedDocsBuildReceipt {
            schema_version: &self.schema_version,
            generator: &self.generator,
            service: &self.service,
            publication_mode: &self.publication_mode,
            producer: &self.producer,
            authority_scope: &self.authority_scope,
            contract_sha256: &self.contract_sha256,
            provenance: &self.provenance,
            artifact_sha256: &self.artifact_sha256,
        };
        let bytes = serde_json::to_vec(&unsigned)?;

        return Ok(sha256_hex(&bytes));
    }
}

fn artifact_digests(bundle: &DocsPublicationBundle) -> BTreeMap<String, String> {
    return bundle
        .files
        .iter()
        .map(|(path, contents)| (path.clone(), sha256_hex(contents.as_bytes())))
        .collect();
}

fn first_artifact_difference(
    expected: &BTreeMap<String, String>,
    observed: &BTreeMap<String, String>,
) -> String {
    for path in expected.keys().chain(observed.keys()) {
        if expected.get(path) != observed.get(path) {
            return path.clone();
        }
    }

    return "<artifact-set>".to_owned();
}

fn valid_sha256(value: &str) -> bool {
    return value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
}

fn valid_git_sha(value: &str) -> bool {
    return matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
}

fn valid_repository(value: &str) -> bool {
    let Some((owner, repo)) = value.split_once('/') else {
        return false;
    };
    if repo.contains('/') {
        return false;
    }

    return valid_identity_segment(owner) && valid_identity_segment(repo);
}

fn valid_identity_segment(value: &str) -> bool {
    return !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
}

fn valid_text(value: &str, max_len: usize) -> bool {
    return !value.is_empty()
        && value.len() <= max_len
        && !value.bytes().any(|byte| byte.is_ascii_control());
}

fn valid_artifact_path(value: &str) -> bool {
    return valid_text(value, 512)
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{render_docs_publication, Catalog, PublicationMode, RouteMap};

    fn publication(mode: PublicationMode) -> DocsPublicationBundle {
        let map = RouteMap::from_json_str(include_str!(
            "../../conformance/docs-publication/scintilla-run.route-map.json"
        ))
        .expect("route map");
        let catalog = Catalog::from_map_with_language(map, None).expect("catalog");
        return render_docs_publication(&catalog, mode, "scintilla-run").expect("publication");
    }

    fn provenance() -> DocsBuildProvenance {
        return DocsBuildProvenance {
            source_repository: "scintilla-run/scintilla-infra".to_owned(),
            source_ref: "refs/heads/main".to_owned(),
            source_sha: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            typespec_sha256: "a".repeat(64),
            json_schema_sha256: "b".repeat(64),
            config_sha256: "c".repeat(64),
            generator_commit: "89abcdef0123456789abcdef0123456789abcdef".to_owned(),
            toolchain_identity: "rustc-1.90.0+typespec-1.4.0".to_owned(),
        };
    }

    #[test]
    fn equal_inputs_produce_equal_receipts() {
        let bundle = publication(PublicationMode::PublisherExternal);
        let first = DocsBuildReceipt::from_publication(&bundle, provenance()).expect("first receipt");
        let second = DocsBuildReceipt::from_publication(&bundle, provenance()).expect("second receipt");

        assert_eq!(first, second);
        assert_eq!(first.receipt_sha256.len(), 64);
        assert_eq!(first.artifact_sha256.len(), bundle.files.len());
        first.verify_publication(&bundle).expect("verify publication");
    }

    #[test]
    fn modified_artifact_is_rejected() {
        let mut bundle = publication(PublicationMode::PublisherExternal);
        let receipt = DocsBuildReceipt::from_publication(&bundle, provenance()).expect("receipt");
        bundle
            .files
            .get_mut("api/openapi.json")
            .expect("openapi")
            .push_str(" \n");

        assert!(matches!(
            receipt.verify_publication(&bundle),
            Err(DocsBuildReceiptError::ArtifactMismatch(path)) if path == "api/openapi.json"
        ));
    }

    #[test]
    fn copied_receipt_cannot_change_publication_mode() {
        let publisher = publication(PublicationMode::PublisherExternal);
        let consumer = publication(PublicationMode::ConsumerProject);
        let receipt = DocsBuildReceipt::from_publication(&publisher, provenance()).expect("receipt");

        assert!(matches!(
            receipt.verify_publication(&consumer),
            Err(DocsBuildReceiptError::PublicationMismatch("publication_mode"))
                | Err(DocsBuildReceiptError::PublicationMismatch("authority_scope"))
                | Err(DocsBuildReceiptError::ArtifactMismatch(_))
        ));
    }

    #[test]
    fn receipt_digest_detects_modified_provenance() {
        let bundle = publication(PublicationMode::PublisherExternal);
        let receipt = DocsBuildReceipt::from_publication(&bundle, provenance()).expect("receipt");
        let json = receipt.to_pretty_json().expect("json");
        let modified = json.replace("refs/heads/main", "refs/heads/other");

        assert!(matches!(
            DocsBuildReceipt::from_json_str(&modified),
            Err(DocsBuildReceiptError::ReceiptDigestMismatch)
        ));
    }

    #[test]
    fn typespec_and_json_schema_authorities_share_receipt_literals() {
        let typespec = include_str!("../../idl/typespec/docs-build-receipt.tsp");
        let schema = include_str!("../../json-schema/docs-build-receipt.schema.json");

        for literal in [
            DOCS_BUILD_RECEIPT_SCHEMA_VERSION,
            "publisher_external",
            "consumer_project",
            "platform_external_developers",
            "project_owned",
            "source_repository",
            "source_sha",
            "typespec_sha256",
            "json_schema_sha256",
            "config_sha256",
            "generator_commit",
            "toolchain_identity",
            "artifact_sha256",
            "receipt_sha256",
        ] {
            assert!(typespec.contains(literal), "TypeSpec is missing {literal}");
            assert!(schema.contains(literal), "JSON Schema is missing {literal}");
        }
    }
}
