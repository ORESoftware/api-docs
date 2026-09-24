//! Provider-neutral deployment documentation for lambda-shaped applications.
//!
//! Producers such as `ores-stack`, `bmscl-compiler`, and Scintilla emit the
//! same interchange manifest. `api-docs` owns validation, deterministic
//! normalization and rendering; it does not import producer implementation code.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const LAMBDA_DEPLOYMENT_DOCS_SCHEMA_VERSION: &str = "ores.api-docs.lambda-deployment.v1";

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LambdaDocsProducer {
    OresStack,
    BmsclCompiler,
    Scintilla,
    External,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LambdaDocsTarget {
    Beamscale,
    Scintilla,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LambdaDocsRuntimeFamily {
    Beam,
    Native,
    Interpreted,
    Jvm,
    Dotnet,
    Wasm,
    Oci,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LambdaDocsCarrier {
    BeamProcess,
    NativeProcess,
    OciContainer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LambdaDocsArchitecture {
    Portable,
    X86_64,
    Arm64,
    Wasm32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LambdaFunctionDeploymentDocs {
    pub id: String,
    pub producer: LambdaDocsProducer,
    pub runtime_family: LambdaDocsRuntimeFamily,
    pub runtime_language: String,
    pub carrier: LambdaDocsCarrier,
    pub architecture: LambdaDocsArchitecture,
    pub entrypoint: String,
    pub artifact_sha256: String,
    pub policy_sha256: String,
    pub operation_keys: Vec<String>,
    pub deploy_targets: Vec<LambdaDocsTarget>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LambdaDeploymentDocsManifest {
    pub schema_version: String,
    pub service: String,
    pub contract_sha256: String,
    pub functions: Vec<LambdaFunctionDeploymentDocs>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum LambdaDeploymentDocsError {
    #[error("manifest does not conform to lambda-deployment-docs JSON Schema: {0}")]
    Schema(String),
    #[error("unsupported schemaVersion {0:?}")]
    SchemaVersion(String),
    #[error("duplicate function id {0:?}")]
    DuplicateFunction(String),
    #[error("function {function:?} contains duplicate operation key {operation:?}")]
    DuplicateOperation { function: String, operation: String },
    #[error("function {function:?} contains duplicate deployment target {target:?}")]
    DuplicateTarget { function: String, target: String },
    #[error(
        "function {function:?} advertises BeamScale but is not an admitted BEAM function: {reason}"
    )]
    InvalidBeamscaleTarget { function: String, reason: String },
    #[error("function {0:?} is produced by ores-stack and cannot advertise BeamScale")]
    OresStackTargetsBeamscale(String),
}

impl LambdaDeploymentDocsManifest {
    pub fn parse_json(input: &str) -> Result<Self, LambdaDeploymentDocsError> {
        let value: Value = serde_json::from_str(input)
            .map_err(|error| LambdaDeploymentDocsError::Schema(error.to_string()))?;
        validate_schema_value(&value)?;
        let manifest: Self = serde_json::from_value(value)
            .map_err(|error| LambdaDeploymentDocsError::Schema(error.to_string()))?;
        manifest.validate_semantics()?;
        Ok(manifest)
    }

    pub fn validate_semantics(&self) -> Result<(), LambdaDeploymentDocsError> {
        if self.schema_version != LAMBDA_DEPLOYMENT_DOCS_SCHEMA_VERSION {
            return Err(LambdaDeploymentDocsError::SchemaVersion(
                self.schema_version.clone(),
            ));
        }

        let mut function_ids = BTreeSet::new();
        for function in &self.functions {
            if !function_ids.insert(function.id.as_str()) {
                return Err(LambdaDeploymentDocsError::DuplicateFunction(
                    function.id.clone(),
                ));
            }

            reject_duplicates(
                &function.id,
                &function.operation_keys,
                |function, operation| LambdaDeploymentDocsError::DuplicateOperation {
                    function,
                    operation,
                },
            )?;

            let mut targets = BTreeSet::new();
            for target in &function.deploy_targets {
                if !targets.insert(target) {
                    return Err(LambdaDeploymentDocsError::DuplicateTarget {
                        function: function.id.clone(),
                        target: target_name(target).to_owned(),
                    });
                }
            }

            if function
                .deploy_targets
                .contains(&LambdaDocsTarget::Beamscale)
            {
                if function.producer == LambdaDocsProducer::OresStack {
                    return Err(LambdaDeploymentDocsError::OresStackTargetsBeamscale(
                        function.id.clone(),
                    ));
                }
                if function.runtime_family != LambdaDocsRuntimeFamily::Beam {
                    return Err(LambdaDeploymentDocsError::InvalidBeamscaleTarget {
                        function: function.id.clone(),
                        reason: "runtimeFamily must be beam".to_owned(),
                    });
                }
                if function.carrier != LambdaDocsCarrier::BeamProcess {
                    return Err(LambdaDeploymentDocsError::InvalidBeamscaleTarget {
                        function: function.id.clone(),
                        reason: "carrier must be beam_process".to_owned(),
                    });
                }
                if function.architecture != LambdaDocsArchitecture::Portable {
                    return Err(LambdaDeploymentDocsError::InvalidBeamscaleTarget {
                        function: function.id.clone(),
                        reason: "architecture must be portable".to_owned(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Return a deterministic representation suitable for JSON/Markdown output.
    /// Producer ordering never affects generated docs.
    pub fn normalized(mut self) -> Result<Self, LambdaDeploymentDocsError> {
        self.validate_semantics()?;
        for function in &mut self.functions {
            function.operation_keys.sort();
            function.deploy_targets.sort();
        }
        self.functions.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(self)
    }

    pub fn to_pretty_json(&self) -> Result<String, LambdaDeploymentDocsError> {
        let normalized = self.clone().normalized()?;
        serde_json::to_string_pretty(&normalized)
            .map_err(|error| LambdaDeploymentDocsError::Schema(error.to_string()))
    }

    pub fn to_markdown(&self) -> Result<String, LambdaDeploymentDocsError> {
        let normalized = self.clone().normalized()?;
        let mut out = String::new();
        out.push_str("# Lambda deployment matrix\n\n");
        out.push_str(&format!(
            "Service: `{}`  \n",
            markdown_cell(&normalized.service)
        ));
        out.push_str(&format!(
            "API contract: `{}`\n\n",
            markdown_cell(&normalized.contract_sha256)
        ));
        out.push_str(
            "| Function | Producer | Runtime | Carrier | Architecture | Targets | Operations |\n",
        );
        out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        for function in &normalized.functions {
            let targets = function
                .deploy_targets
                .iter()
                .map(target_name)
                .collect::<Vec<_>>()
                .join(", ");
            let operations = function.operation_keys.join(", ");
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` / `{}` | `{}` | `{}` | {} | {} |\n",
                markdown_cell(&function.id),
                producer_name(&function.producer),
                runtime_family_name(&function.runtime_family),
                markdown_cell(&function.runtime_language),
                carrier_name(&function.carrier),
                architecture_name(&function.architecture),
                markdown_cell(&targets),
                markdown_cell(&operations),
            ));
        }
        Ok(out)
    }
}

fn validate_schema_value(value: &Value) -> Result<(), LambdaDeploymentDocsError> {
    let schema: Value = serde_json::from_str(include_str!(
        "../../json-schema/lambda-deployment-docs.schema.json"
    ))
    .expect("lambda-deployment-docs JSON Schema must be valid JSON");
    let validator = jsonschema::validator_for(&schema)
        .expect("lambda-deployment-docs JSON Schema must compile");
    validator
        .validate(value)
        .map_err(|error| LambdaDeploymentDocsError::Schema(error.to_string()))
}

fn reject_duplicates<F>(
    function: &str,
    values: &[String],
    error: F,
) -> Result<(), LambdaDeploymentDocsError>
where
    F: Fn(String, String) -> LambdaDeploymentDocsError,
{
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value.as_str()) {
            return Err(error(function.to_owned(), value.clone()));
        }
    }
    Ok(())
}

fn producer_name(value: &LambdaDocsProducer) -> &'static str {
    match value {
        LambdaDocsProducer::OresStack => "ores-stack",
        LambdaDocsProducer::BmsclCompiler => "bmscl-compiler",
        LambdaDocsProducer::Scintilla => "scintilla",
        LambdaDocsProducer::External => "external",
    }
}

fn target_name(value: &LambdaDocsTarget) -> &'static str {
    match value {
        LambdaDocsTarget::Beamscale => "beamscale",
        LambdaDocsTarget::Scintilla => "scintilla",
    }
}

fn runtime_family_name(value: &LambdaDocsRuntimeFamily) -> &'static str {
    match value {
        LambdaDocsRuntimeFamily::Beam => "beam",
        LambdaDocsRuntimeFamily::Native => "native",
        LambdaDocsRuntimeFamily::Interpreted => "interpreted",
        LambdaDocsRuntimeFamily::Jvm => "jvm",
        LambdaDocsRuntimeFamily::Dotnet => "dotnet",
        LambdaDocsRuntimeFamily::Wasm => "wasm",
        LambdaDocsRuntimeFamily::Oci => "oci",
    }
}

fn carrier_name(value: &LambdaDocsCarrier) -> &'static str {
    match value {
        LambdaDocsCarrier::BeamProcess => "beam_process",
        LambdaDocsCarrier::NativeProcess => "native_process",
        LambdaDocsCarrier::OciContainer => "oci_container",
    }
}

fn architecture_name(value: &LambdaDocsArchitecture) -> &'static str {
    match value {
        LambdaDocsArchitecture::Portable => "portable",
        LambdaDocsArchitecture::X86_64 => "x86_64",
        LambdaDocsArchitecture::Arm64 => "arm64",
        LambdaDocsArchitecture::Wasm32 => "wasm32",
    }
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn fixture() -> String {
        format!(
            r#"{{
  "schemaVersion": "ores.api-docs.lambda-deployment.v1",
  "service": "demo-api",
  "contractSha256": "{SHA}",
  "functions": [
    {{
      "id": "z.beam",
      "producer": "bmscl-compiler",
      "runtimeFamily": "beam",
      "runtimeLanguage": "gleam",
      "carrier": "beam_process",
      "architecture": "portable",
      "entrypoint": "worker:handle/2",
      "artifactSha256": "{SHA}",
      "policySha256": "{SHA}",
      "operationKeys": ["demo.z", "demo.a"],
      "deployTargets": ["scintilla", "beamscale"]
    }},
    {{
      "id": "a.rust",
      "producer": "ores-stack",
      "runtimeFamily": "native",
      "runtimeLanguage": "rust",
      "carrier": "native_process",
      "architecture": "x86_64",
      "entrypoint": "service",
      "artifactSha256": "{SHA}",
      "policySha256": "{SHA}",
      "operationKeys": ["demo.run"],
      "deployTargets": ["scintilla"]
    }}
  ]
}}"#
        )
    }

    #[test]
    fn normalizes_functions_operations_and_targets() {
        let manifest = LambdaDeploymentDocsManifest::parse_json(&fixture()).expect("fixture");
        let normalized = manifest.normalized().expect("normalize");
        assert_eq!(normalized.functions[0].id, "a.rust");
        assert_eq!(
            normalized.functions[1].operation_keys,
            vec!["demo.a".to_owned(), "demo.z".to_owned()]
        );
        assert_eq!(
            normalized.functions[1].deploy_targets,
            vec![LambdaDocsTarget::Beamscale, LambdaDocsTarget::Scintilla]
        );
    }

    #[test]
    fn ores_stack_cannot_claim_beamscale() {
        let invalid = fixture().replace(
            "\"deployTargets\": [\"scintilla\"]",
            "\"deployTargets\": [\"beamscale\", \"scintilla\"]",
        );
        let error = LambdaDeploymentDocsManifest::parse_json(&invalid).unwrap_err();
        assert!(matches!(
            error,
            LambdaDeploymentDocsError::OresStackTargetsBeamscale(_)
        ));
    }

    #[test]
    fn beamscale_requires_portable_beam_process() {
        let invalid = fixture().replace(
            "\"runtimeFamily\": \"beam\"",
            "\"runtimeFamily\": \"native\"",
        );
        let error = LambdaDeploymentDocsManifest::parse_json(&invalid).unwrap_err();
        assert!(matches!(
            error,
            LambdaDeploymentDocsError::InvalidBeamscaleTarget { .. }
        ));
    }

    #[test]
    fn markdown_is_stable_and_mentions_both_targets() {
        let manifest = LambdaDeploymentDocsManifest::parse_json(&fixture()).expect("fixture");
        let markdown = manifest.to_markdown().expect("markdown");
        assert!(markdown.contains("`a.rust`"));
        assert!(markdown.contains("beamscale, scintilla"));
        assert!(markdown.find("`a.rust`").unwrap() < markdown.find("`z.beam`").unwrap());
    }
}
