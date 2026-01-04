//! openEHR wire/boundary support.
//!
//! This crate is responsible for translating between on-disk, Git-backed clinical file formats
//! (YAML and Markdown with YAML front matter) and VPR internal record components.
//!
//! Clinical meaning lives in `vpr-core` under `vpr_core::components`. This crate handles file
//! formats and standards alignment only.

pub mod rm_1_1_0;
pub mod vpr;

use thiserror::Error;

/// Errors returned by the `openehr` boundary crate.
#[derive(Debug, Error)]
pub enum OpenehrError {
    #[error("invalid YAML: {0}")]
    InvalidYaml(#[from] serde_yaml::Error),

    #[error("missing YAML front matter header (expected '---' as first line)")]
    MissingFrontMatter,

    #[error("unterminated YAML front matter (missing closing '---' line)")]
    UnterminatedFrontMatter,

    #[error("front matter must be a YAML mapping")]
    FrontMatterNotMapping,

    #[error("invalid UTF-8 or text structure")]
    InvalidText,

    #[error("translation error: {0}")]
    Translation(String),
}

/// Read an RM 1.1.0 `EHR_STATUS` component from YAML.
pub fn read_ehr_status_yaml(yaml: &str) -> Result<rm_1_1_0::ehr_status::EhrStatus, OpenehrError> {
    rm_1_1_0::ehr_status::read_yaml(yaml)
}

/// Write an RM 1.1.0 `EHR_STATUS` component to YAML.
pub fn write_ehr_status_yaml(
    component: &rm_1_1_0::ehr_status::EhrStatus,
) -> Result<String, OpenehrError> {
    rm_1_1_0::ehr_status::write_yaml(component)
}

/// Read an RM 1.1.0 narrative component from Markdown with YAML front matter.
pub fn read_narrative_markdown(
    input: &str,
) -> Result<rm_1_1_0::narrative::NarrativeComponent, OpenehrError> {
    rm_1_1_0::narrative::read_markdown(input)
}

/// Write an RM 1.1.0 narrative component to Markdown with YAML front matter.
pub fn write_narrative_markdown(
    component: &rm_1_1_0::narrative::NarrativeComponent,
) -> Result<String, OpenehrError> {
    rm_1_1_0::narrative::write_markdown(component)
}
