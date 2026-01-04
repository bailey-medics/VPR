use crate::OpenehrError;
use serde::{Deserialize, Serialize};

/// RM 1.1.0 narrative component (wire model).
///
/// This exists purely to support reading/writing Markdown with YAML front matter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NarrativeComponent {
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub body: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NarrativeFrontMatterWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
}

/// Read an RM 1.1.0 narrative component from Markdown with YAML front matter.
pub(crate) fn read_markdown(input: &str) -> Result<NarrativeComponent, OpenehrError> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let (front_matter, body) = split_yaml_front_matter(input)?;

    let metadata = if front_matter.trim().is_empty() {
        NarrativeFrontMatterWire::default()
    } else {
        let yaml_value: serde_yaml::Value = serde_yaml::from_str(front_matter)?;
        if !matches!(yaml_value, serde_yaml::Value::Mapping(_)) {
            return Err(OpenehrError::FrontMatterNotMapping);
        }
        serde_yaml::from_value(yaml_value)?
    };

    Ok(NarrativeComponent {
        title: metadata.title,
        tags: metadata.tags,
        body: body.to_string(),
    })
}

/// Write an RM 1.1.0 narrative component to Markdown with YAML front matter.
pub(crate) fn write_markdown(component: &NarrativeComponent) -> Result<String, OpenehrError> {
    let metadata = NarrativeFrontMatterWire {
        title: component.title.clone(),
        tags: component.tags.clone(),
    };

    let mut out = String::new();
    out.push_str("---\n");
    let yaml = serde_yaml::to_string(&metadata)?;
    out.push_str(&yaml);
    if !yaml.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("---\n");
    out.push_str(&component.body);
    Ok(out)
}

fn split_yaml_front_matter(input: &str) -> Result<(&str, &str), OpenehrError> {
    let mut chunks = input.split_inclusive('\n');

    let first = chunks.next().ok_or(OpenehrError::InvalidText)?;
    let first_line = first.trim_end_matches(['\n', '\r']);
    if first_line != "---" {
        return Err(OpenehrError::MissingFrontMatter);
    }

    let mut offset = first.len();

    // Find closing delimiter line and return slices into the original input.
    for chunk in chunks {
        let line = chunk.trim_end_matches(['\n', '\r']);
        if line == "---" {
            let front = &input[first.len()..offset];
            let body = &input[offset + chunk.len()..];
            return Ok((front, body));
        }
        offset += chunk.len();
    }

    Err(OpenehrError::UnterminatedFrontMatter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_front_matter_and_body() {
        let input = "---\ntitle: Consult note\ntags: [gp, follow-up]\n---\n# Heading\nBody text\n";
        let component = read_markdown(input).expect("parse");
        assert_eq!(component.title.as_deref(), Some("Consult note"));
        assert_eq!(component.tags, vec!["gp", "follow-up"]);
        assert!(component.body.starts_with('#'));
    }

    #[test]
    fn rejects_missing_front_matter() {
        let err = read_markdown("# No front matter").unwrap_err();
        assert!(matches!(err, OpenehrError::MissingFrontMatter));
    }
}
