//! Markdown validation and sanitisation for coordination thread messages.
//!
//! Provides utilities to construct properly formatted markdown from user-provided prose,
//! escaping formatting characters that could interfere with message display. Headers and
//! metadata are validated and formatted, while prose content is sanitised to prevent
//! unintended markdown interpretation.

use crate::error::{PatientError, PatientResult};

/// Service for markdown validation and sanitisation.
#[derive(Debug, Clone)]
pub struct MarkdownService;

impl MarkdownService {
    /// Creates a new `MarkdownService` instance.
    pub fn new() -> Self {
        Self
    }

    /// Validates and sanitises markdown content for coordination thread messages.
    ///
    /// Constructs properly formatted markdown from optional header, metadata variables,
    /// and prose content. Validates header format and escapes markdown syntax in prose
    /// to prevent unintended formatting whilst preserving readability.
    ///
    /// Prose escaping rules:
    /// - `#` at line start → `\#` (prevents headers)
    /// - Triple backticks → `\`\`\`` (prevents code blocks)
    /// - Standalone `---`, `***`, `___` → escaped (prevents horizontal rules)
    ///
    /// # Arguments
    ///
    /// * `header` - Optional markdown header (must start with '#')
    /// * `variables` - Metadata pairs formatted as `**Key:** Value` lines
    /// * `prose` - User-provided content with markdown syntax escaped (must not be empty)
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if:
    /// - Header is provided but does not start with '#'
    /// - Prose is empty
    pub fn message_prepare(
        &self,
        header: Option<&str>,
        variables: Vec<(&str, &str)>,
        prose: &str,
    ) -> PatientResult<String> {
        // Validate prose is not empty
        if prose.trim().is_empty() {
            return Err(PatientError::InvalidInput(
                "Prose content must not be empty".to_string(),
            ));
        }

        let mut output = String::new();

        // Validate and add header
        if let Some(h) = header {
            let trimmed = h.trim();
            if !trimmed.is_empty() {
                if !trimmed.starts_with('#') {
                    return Err(PatientError::InvalidInput(
                        "Header must start with '#' for markdown formatting".to_string(),
                    ));
                }
                output.push_str(trimmed);
                output.push_str("\n\n");
            }
        }

        // Format variables as bold key-value pairs
        if !variables.is_empty() {
            for (key, value) in &variables {
                output.push_str(&format!("**{}:** {}\n", key, value));
            }
            output.push('\n');
        }

        // Escape and append prose
        let sanitised_prose = self.escape_prose(prose);
        output.push_str(&sanitised_prose);

        Ok(output)
    }

    /// Escapes markdown formatting characters in prose content.
    ///
    /// Prevents unintended markdown interpretation by escaping line-start `#`,
    /// triple backticks, and standalone horizontal rule patterns.
    ///
    /// # Arguments
    ///
    /// * `prose` - User-provided text content to escape
    ///
    /// # Returns
    ///
    /// Escaped prose with markdown syntax characters preceded by backslashes.
    fn escape_prose(&self, prose: &str) -> String {
        let mut result = String::new();
        let lines: Vec<&str> = prose.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Escape lines starting with #
            let escaped_line = if line.trim_start().starts_with('#') {
                line.replacen('#', r"\#", 1)
            } else if trimmed == "---" || trimmed == "***" || trimmed == "___" {
                // Escape horizontal rules
                format!(r"\{}", trimmed)
            } else {
                // Escape triple backticks anywhere in the line
                line.replace("```", r"\`\`\`")
            };

            result.push_str(&escaped_line);

            // Add newline unless it's the last line
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        result
    }

    /// Unescapes markdown formatting characters in prose content.
    ///
    /// Reverses escaping applied by `escape_prose`, restoring original markdown
    /// syntax characters for display or further processing.
    ///
    /// # Arguments
    ///
    /// * `prose` - Escaped text content to restore
    ///
    /// # Returns
    ///
    /// Unescaped prose with backslash escapes removed from markdown syntax.
    pub fn unescape_prose(&self, prose: &str) -> String {
        let mut result = String::new();
        let lines: Vec<&str> = prose.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Unescape lines starting with \#
            let unescaped_line = if line.trim_start().starts_with(r"\#") {
                line.replacen(r"\#", "#", 1)
            } else if trimmed == r"\---" || trimmed == r"\***" || trimmed == r"\___" {
                // Unescape horizontal rules
                trimmed.trim_start_matches('\\').to_string()
            } else {
                // Unescape triple backticks
                line.replace(r"\`\`\`", "```")
            };

            result.push_str(&unescaped_line);

            // Add newline unless it's the last line
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        result
    }
}

impl Default for MarkdownService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_prepare_with_header_and_prose() {
        let service = MarkdownService::new();
        let result = service
            .message_prepare(Some("# Title"), vec![], "Plain text content")
            .unwrap();
        assert_eq!(result, "# Title\n\nPlain text content");
    }

    #[test]
    fn test_message_prepare_escapes_hash_in_prose() {
        let service = MarkdownService::new();
        let prose = "Patient #12345 has condition\n# This should be escaped";
        let result = service.message_prepare(None, vec![], prose).unwrap();
        assert_eq!(
            result,
            "Patient #12345 has condition\n\\# This should be escaped"
        );
    }

    #[test]
    fn test_message_prepare_escapes_code_blocks() {
        let service = MarkdownService::new();
        let prose = "Example code: ```python\nprint('hello')\n```";
        let result = service.message_prepare(None, vec![], prose).unwrap();
        assert_eq!(
            result,
            "Example code: \\`\\`\\`python\nprint('hello')\n\\`\\`\\`"
        );
    }

    #[test]
    fn test_message_prepare_escapes_horizontal_rules() {
        let service = MarkdownService::new();
        let prose = "Line 1\n---\nLine 2\n***\nLine 3\n___";
        let result = service.message_prepare(None, vec![], prose).unwrap();
        assert_eq!(result, "Line 1\n\\---\nLine 2\n\\***\nLine 3\n\\___");
    }

    #[test]
    fn test_message_prepare_with_variables() {
        let service = MarkdownService::new();
        let variables = vec![("Status", "Active"), ("Priority", "High")];
        let result = service
            .message_prepare(None, variables, "Some content")
            .unwrap();
        assert_eq!(
            result,
            "**Status:** Active\n**Priority:** High\n\nSome content"
        );
    }

    #[test]
    fn test_message_prepare_header_without_hash_fails() {
        let service = MarkdownService::new();
        let result = service.message_prepare(Some("Not a header"), vec![], "content");
        assert!(result.is_err());
    }
    #[test]
    fn test_message_prepare_empty_prose_fails() {
        let service = MarkdownService::new();
        let result = service.message_prepare(None, vec![], "");
        assert!(result.is_err());
        let result2 = service.message_prepare(None, vec![], "   ");
        assert!(result2.is_err());
    }
    #[test]
    fn test_message_prepare_full_example() {
        let service = MarkdownService::new();
        let variables = vec![("Patient ID", "12345"), ("Severity", "Medium")];
        let prose = "Patient #12345 presented with symptoms.\n# Important note\nSee details.";

        let result = service
            .message_prepare(Some("## Clinical Update"), variables, prose)
            .unwrap();

        let expected = "## Clinical Update\n\n\
                       **Patient ID:** 12345\n\
                       **Severity:** Medium\n\n\
                       Patient #12345 presented with symptoms.\n\
                       \\# Important note\n\
                       See details.";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_unescape_prose_hash() {
        let service = MarkdownService::new();
        let escaped = "Patient #12345 has condition\n\\# This was escaped";
        let result = service.unescape_prose(escaped);
        assert_eq!(result, "Patient #12345 has condition\n# This was escaped");
    }

    #[test]
    fn test_unescape_prose_code_blocks() {
        let service = MarkdownService::new();
        let escaped = "Example code: \\`\\`\\`python\nprint('hello')\n\\`\\`\\`";
        let result = service.unescape_prose(escaped);
        assert_eq!(result, "Example code: ```python\nprint('hello')\n```");
    }

    #[test]
    fn test_unescape_prose_horizontal_rules() {
        let service = MarkdownService::new();
        let escaped = "Line 1\n\\---\nLine 2\n\\***\nLine 3\n\\___";
        let result = service.unescape_prose(escaped);
        assert_eq!(result, "Line 1\n---\nLine 2\n***\nLine 3\n___");
    }

    #[test]
    fn test_escape_unescape_roundtrip() {
        let service = MarkdownService::new();
        let original = "Patient #12345\n# Important\n---\n```code```";
        let escaped = service.escape_prose(original);
        let unescaped = service.unescape_prose(&escaped);
        assert_eq!(unescaped, original);
    }
}
