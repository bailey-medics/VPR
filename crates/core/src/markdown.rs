//! Markdown validation and sanitisation for coordination thread messages.
//!
//! Provides utilities to construct properly formatted markdown from user-provided prose,
//! escaping formatting characters that could interfere with message display. Headers and
//! metadata are validated and formatted, while prose content is sanitised to prevent
//! unintended markdown interpretation.

use crate::error::{PatientError, PatientResult};
use crate::NonEmptyText;
use chrono::{DateTime, Utc};
use fhir::{AuthorRole, MessageAuthor};
use uuid::Uuid;

/// Thread header used for all coordination threads.
const THREAD_HEADER: &str = "# Thread";

/// Generic message structure for parsing.
///
/// Each message contains strongly-typed metadata and body content.
/// Messages are typically separated by horizontal rules in the markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Strongly-typed message metadata
    pub metadata: MessageMetadata,
    /// Main body content (unescaped)
    pub body: NonEmptyText,
    /// Optional UUID of message being corrected
    pub corrects: Option<Uuid>,
}

/// Represents a single message within a coordination thread with strong typing.
///
/// Contains structured metadata only. Body content is kept separate.
/// Messages are typically separated by horizontal rules in the markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageMetadata {
    /// Unique identifier for this message
    pub message_id: Uuid,
    /// ISO 8601 timestamp when the message was created
    pub timestamp: DateTime<Utc>,
    /// Message author with identity, name, and role
    pub author: MessageAuthor,
}

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
    /// Constructs properly formatted markdown from message metadata and body content.
    /// Escapes markdown syntax in body to prevent unintended formatting whilst preserving
    /// readability.
    ///
    /// Body escaping rules:
    /// - `#` at line start → `\#` (prevents headers)
    /// - Triple backticks → `\`\`\`` (prevents code blocks)
    /// - Standalone `---`, `***`, `___` → escaped (prevents horizontal rules)
    ///
    /// Message format produced:
    /// ```markdown
    /// **Message ID:** <uuid>
    /// **Timestamp:** <iso8601>
    /// **Author ID:** <uuid>
    /// **Author name:** <name>
    /// **Author role:** <role>
    /// **Corrects:** <uuid> (optional)
    ///
    /// Body content here
    /// ```
    ///
    /// # Arguments
    ///
    /// * `metadata` - Message metadata containing ID, timestamp, and author details
    /// * `body` - User-provided content to be escaped (guaranteed non-empty by type)
    /// * `corrects` - Optional UUID of message being corrected
    ///
    /// # Returns
    ///
    /// Formatted markdown message with metadata and escaped body content.
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if role serialization fails.
    pub fn message_render(
        &self,
        metadata: &MessageMetadata,
        body: &NonEmptyText,
        corrects: Option<Uuid>,
    ) -> PatientResult<NonEmptyText> {
        let mut output = String::new();

        // Format metadata as bold key-value pairs
        output.push_str(&format!("**Message ID:** {}\n", metadata.message_id));
        output.push_str(&format!(
            "**Timestamp:** {}\n",
            metadata
                .timestamp
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ));
        output.push_str(&format!("**Author ID:** {}\n", metadata.author.id));
        output.push_str(&format!(
            "**Author name:** {}\n",
            metadata.author.name.as_str()
        ));

        let role_str = serde_json::to_string(&metadata.author.role)
            .map_err(|e| PatientError::InvalidInput(format!("Invalid role: {}", e)))?
            .trim_matches('"')
            .to_string();
        output.push_str(&format!("**Author role:** {}\n", role_str));

        if let Some(corrects_id) = corrects {
            output.push_str(&format!("**Corrects:** {}\n", corrects_id));
        }

        output.push('\n');

        // Escape and append body
        let sanitised_body = self.escape_body(body.as_str());
        output.push_str(sanitised_body.as_str());

        // Add blank line after body, then separator for message delimiting
        output.push_str("\n\n---\n");

        NonEmptyText::new(output).map_err(|e| PatientError::InvalidInput(e.to_string()))
    }

    /// Renders multiple messages into a complete markdown thread.
    ///
    /// Takes a collection of messages and renders them all to markdown format.
    /// Each message is rendered with its metadata and body, separated by horizontal rules.
    /// The thread starts with a level-1 header "# Thread".
    ///
    /// # Arguments
    ///
    /// * `messages` - Slice of messages to render
    ///
    /// # Returns
    ///
    /// Complete markdown content with all messages rendered and separated.
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if any message has an empty body.
    pub fn thread_render(&self, messages: &[Message]) -> PatientResult<NonEmptyText> {
        let rendered_messages = messages
            .iter()
            .map(|msg| self.message_render(&msg.metadata, &msg.body, msg.corrects))
            .collect::<PatientResult<Vec<NonEmptyText>>>()?;

        let content = rendered_messages
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("");

        NonEmptyText::new(format!("{}\n\n{}\n", THREAD_HEADER, content.trim_end()))
            .map_err(|e| PatientError::InvalidInput(e.to_string()))
    }

    /// Parses a markdown thread file into structured messages.
    ///
    /// Reads a complete coordination thread from markdown format, splitting on horizontal
    /// rules (`---`) to separate individual messages. Each message is parsed to extract
    /// variables and body content with markdown escaping reversed. The thread-level
    /// header (level-1) is ignored as it's not part of individual messages.
    ///
    /// Thread format expected:
    /// ```markdown
    /// # Thread Title
    ///
    /// **Variable1:** Value1
    /// **Variable2:** Value2
    ///
    /// Body content here
    ///
    /// ---
    ///
    /// **Variable1:** Value1
    ///
    /// Next message
    /// ...
    /// ```
    ///
    /// # Arguments
    ///
    /// * `content` - Complete markdown content from messages.md file
    ///
    /// # Returns
    ///
    /// Vector of parsed `Message` structures with strong typing.
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if content cannot be parsed correctly.
    pub fn thread_parse(&self, content: &str) -> PatientResult<Vec<Message>> {
        if content.trim().is_empty() {
            return Err(PatientError::InvalidInput(
                "Thread content must not be empty".to_string(),
            ));
        }

        let mut messages = Vec::new();

        // Split by horizontal rules (---) that appear on their own line
        // Use regex-like pattern to handle variable whitespace: split on newline(s), ---, newline(s)
        // First normalize multiple newlines around --- to make splitting consistent
        let normalized = content.trim();
        let raw_sections: Vec<&str> = normalized
            .split("---")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        for section in raw_sections {
            let message = self.message_parse(section)?;
            messages.push(message);
        }

        Ok(messages)
    }

    /// Parses a single message section into a structured `Message`.
    ///
    /// Extracts variables (lines matching `**Key:** Value`) and body content.
    /// Unescapes markdown syntax in body for display. Skips level-1 headers
    /// as they belong to the thread, not individual messages.
    ///
    /// # Arguments
    ///
    /// * `section` - Single message content (between horizontal rules or from start)
    ///
    /// # Returns
    ///
    /// Parsed `Message` with metadata and unescaped body.
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if message format is invalid.
    fn message_parse(&self, section: &str) -> PatientResult<Message> {
        let lines: Vec<&str> = section.lines().collect();

        let mut variables: Vec<(NonEmptyText, NonEmptyText)> = Vec::new();
        let mut body_lines: Vec<&str> = Vec::new();

        // State tracking: 0 = looking for variables, 1 = in body
        let mut state = 0;
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim();

            // Skip level-1 headers (thread title) and following blank line
            if trimmed.starts_with("# ") && !trimmed.starts_with("##") {
                i += 1;
                if i < lines.len() && lines[i].trim().is_empty() {
                    i += 1;
                }
                continue;
            }

            // State 0: Looking for variables
            if state == 0 {
                if trimmed.starts_with("**") && trimmed.contains(":**") {
                    if let Some(colon_pos) = trimmed.find(":**") {
                        let key = NonEmptyText::new(trimmed[2..colon_pos].trim())
                            .map_err(|e| PatientError::InvalidInput(e.to_string()))?;
                        let value = NonEmptyText::new(trimmed[colon_pos + 3..].trim())
                            .map_err(|e| PatientError::InvalidInput(e.to_string()))?;
                        variables.push((key, value));
                        i += 1;
                        continue;
                    }
                } else if trimmed.is_empty() && !variables.is_empty() {
                    // Blank line after variables, transition to body
                    state = 1;
                    i += 1;
                    continue;
                } else if trimmed.is_empty() {
                    // Skip leading blank lines
                    i += 1;
                    continue;
                } else if !trimmed.is_empty() {
                    // Non-variable content, must be body starting
                    state = 1;
                }
            }

            // State 1: Collecting body
            if state == 1 {
                body_lines.push(line);
                i += 1;
            } else {
                i += 1;
            }
        }

        // Join body lines and unescape
        let body_text = body_lines.join("\n");
        if body_text.trim().is_empty() {
            return Err(PatientError::InvalidInput(
                "Message must contain body content".to_string(),
            ));
        }
        let unescaped = self.unescape_body(&body_text);
        let body = NonEmptyText::new(unescaped)
            .map_err(|_| PatientError::InvalidInput("Body cannot be empty".to_string()))?;

        // Parse variables into MessageMetadata
        let mut message_id = None;
        let mut timestamp = None;
        let mut author_id = None;
        let mut author_name = None;
        let mut author_role = None;
        let mut corrects = None;

        for (key, value) in &variables {
            match key.as_str() {
                "Message ID" => message_id = Uuid::parse_str(value.as_str()).ok(),
                "Timestamp" => {
                    timestamp = DateTime::parse_from_rfc3339(value.as_str())
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                }
                "Author ID" => author_id = Uuid::parse_str(value.as_str()).ok(),
                "Author name" => author_name = Some(value.clone()),
                "Author" => author_name = Some(value.clone()), // Legacy support
                "Author role" => {
                    author_role = AuthorRole::parse(value.as_str())
                        .map_err(|e| PatientError::InvalidInput(e.to_string()))
                        .ok();
                }
                "Role" => {
                    // Legacy support
                    author_role = AuthorRole::parse(value.as_str())
                        .map_err(|e| PatientError::InvalidInput(e.to_string()))
                        .ok();
                }
                "Corrects" => corrects = Uuid::parse_str(value.as_str()).ok(),
                _ => {}
            }
        }

        let metadata = MessageMetadata {
            message_id: message_id.ok_or_else(|| {
                PatientError::InvalidInput("Missing or invalid Message ID".to_string())
            })?,
            timestamp: timestamp.ok_or_else(|| {
                PatientError::InvalidInput("Missing or invalid Timestamp".to_string())
            })?,
            author: MessageAuthor {
                id: author_id.ok_or_else(|| {
                    PatientError::InvalidInput("Missing or invalid Author ID".to_string())
                })?,
                name: author_name
                    .ok_or_else(|| PatientError::InvalidInput("Missing Author".to_string()))?,
                role: author_role.ok_or_else(|| {
                    PatientError::InvalidInput("Missing or invalid Role".to_string())
                })?,
            },
        };

        Ok(Message {
            metadata,
            body,
            corrects,
        })
    }
}

// Helper methods for escaping and unescaping body content
impl MarkdownService {
    /// Escapes markdown formatting characters in body content.
    ///
    /// Prevents unintended markdown interpretation by escaping line-start `#`,
    /// triple backticks, and standalone horizontal rule patterns.
    ///
    /// # Arguments
    ///
    /// * `body` - User-provided text content to escape
    ///
    /// # Returns
    ///
    /// Escaped body with markdown syntax characters preceded by backslashes.
    fn escape_body(&self, body: &str) -> NonEmptyText {
        let mut result = String::new();
        let lines: Vec<&str> = body.lines().collect();

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

        NonEmptyText::new(result).expect("escaped body should be non-empty")
    }

    /// Unescapes markdown formatting characters in body content.
    ///
    /// Reverses escaping applied by `escape_body`, restoring original markdown
    /// syntax characters for display or further processing.
    ///
    /// # Arguments
    ///
    /// * `body` - Escaped text content to restore
    ///
    /// # Returns
    ///
    /// Unescaped body with backslash escapes removed from markdown syntax.
    fn unescape_body(&self, body: &str) -> NonEmptyText {
        let mut result = String::new();
        let lines: Vec<&str> = body.lines().collect();

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

        NonEmptyText::new(result).expect("unescaped body should be non-empty")
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
    fn test_thread_start_with_body() {
        let service = MarkdownService::new();
        let metadata = MessageMetadata {
            message_id: Uuid::nil(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-22T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            author: MessageAuthor {
                id: Uuid::nil(),
                name: NonEmptyText::new("Test Author").unwrap(),
                role: AuthorRole::Clinician,
            },
        };
        let msg = Message {
            metadata,
            body: NonEmptyText::new("Plain text content").unwrap(),
            corrects: None,
        };
        let result = service.thread_render(&[msg]).unwrap();
        assert!(result.as_str().starts_with("# Thread\n\n"));
        assert!(result.as_str().contains("Plain text content"));
    }

    #[test]
    fn test_message_prepare_escapes_hash_in_body() {
        let service = MarkdownService::new();
        let body =
            NonEmptyText::new("Patient #12345 has condition\n# This should be escaped").unwrap();
        let metadata = MessageMetadata {
            message_id: Uuid::nil(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-22T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            author: MessageAuthor {
                id: Uuid::nil(),
                name: NonEmptyText::new("Test Author").unwrap(),
                role: AuthorRole::Clinician,
            },
        };
        let result = service.message_render(&metadata, &body, None).unwrap();
        assert!(result
            .as_str()
            .contains("Patient #12345 has condition\n\\# This should be escaped"));
    }

    #[test]
    fn test_message_prepare_escapes_code_blocks() {
        let service = MarkdownService::new();
        let body = NonEmptyText::new("Example code: ```python\nprint('hello')\n```").unwrap();
        let metadata = MessageMetadata {
            message_id: Uuid::nil(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-22T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            author: MessageAuthor {
                id: Uuid::nil(),
                name: NonEmptyText::new("Test Author").unwrap(),
                role: AuthorRole::Clinician,
            },
        };
        let result = service.message_render(&metadata, &body, None).unwrap();
        assert!(result
            .as_str()
            .contains("Example code: \\`\\`\\`python\nprint('hello')\n\\`\\`\\`"));
    }

    #[test]
    fn test_message_prepare_escapes_horizontal_rules() {
        let service = MarkdownService::new();
        let body = NonEmptyText::new("Line 1\n---\nLine 2\n***\nLine 3\n___").unwrap();
        let metadata = MessageMetadata {
            message_id: Uuid::nil(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-22T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            author: MessageAuthor {
                id: Uuid::nil(),
                name: NonEmptyText::new("Test Author").unwrap(),
                role: AuthorRole::Clinician,
            },
        };
        let result = service.message_render(&metadata, &body, None).unwrap();
        assert!(result.as_str().contains("Line 1\n\\---"));
        assert!(result.as_str().contains("Line 2\n\\***"));
        assert!(result.as_str().contains("Line 3\n\\___"));
    }

    #[test]
    fn test_message_prepare_with_metadata() {
        let service = MarkdownService::new();
        let metadata = MessageMetadata {
            message_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-22T10:30:00.123Z")
                .unwrap()
                .with_timezone(&Utc),
            author: MessageAuthor {
                id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap(),
                name: NonEmptyText::new("Dr Smith").unwrap(),
                role: AuthorRole::Clinician,
            },
        };
        let result = service
            .message_render(&metadata, &NonEmptyText::new("Some content").unwrap(), None)
            .unwrap();
        assert!(result
            .as_str()
            .contains("**Message ID:** 550e8400-e29b-41d4-a716-446655440000"));
        assert!(result.as_str().contains("**Author name:** Dr Smith"));
        assert!(result.as_str().contains("**Author role:** clinician"));
        assert!(result.as_str().contains("Some content"));
    }

    #[test]
    fn test_message_render_empty_body_fails() {
        // NonEmptyText type prevents empty strings at construction time
        let result = NonEmptyText::new("");
        assert!(result.is_err());
        let result2 = NonEmptyText::new("   ");
        assert!(result2.is_err());
    }
    #[test]
    fn test_thread_start_full_example() {
        let service = MarkdownService::new();
        let body = NonEmptyText::new(
            "Patient #12345 presented with symptoms.\n# Important note\nSee details.",
        )
        .unwrap();
        let metadata = MessageMetadata {
            message_id: Uuid::nil(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-22T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            author: MessageAuthor {
                id: Uuid::nil(),
                name: NonEmptyText::new("Test Author").unwrap(),
                role: AuthorRole::Clinician,
            },
        };

        let msg = Message {
            metadata,
            body: body.clone(),
            corrects: None,
        };
        let result = service.thread_render(&[msg]).unwrap();

        assert!(result.as_str().starts_with("# Thread\n\n"));
        assert!(result
            .as_str()
            .contains("Patient #12345 presented with symptoms."));
        assert!(result.as_str().contains("\\# Important note"));
        assert!(result.as_str().contains("See details."));
    }

    #[test]
    fn test_unescape_body_hash() {
        let service = MarkdownService::new();
        let escaped = "Patient #12345 has condition\n\\# This was escaped";
        let result = service.unescape_body(escaped);
        assert_eq!(
            result.as_str(),
            "Patient #12345 has condition\n# This was escaped"
        );
    }

    #[test]
    fn test_unescape_body_code_blocks() {
        let service = MarkdownService::new();
        let escaped = "Example code: \\`\\`\\`python\nprint('hello')\n\\`\\`\\`";
        let result = service.unescape_body(escaped);
        assert_eq!(
            result.as_str(),
            "Example code: ```python\nprint('hello')\n```"
        );
    }

    #[test]
    fn test_unescape_body_horizontal_rules() {
        let service = MarkdownService::new();
        let escaped = "Line 1\n\\---\nLine 2\n\\***\nLine 3\n\\___";
        let result = service.unescape_body(escaped);
        assert_eq!(result.as_str(), "Line 1\n---\nLine 2\n***\nLine 3\n___");
    }

    #[test]
    fn test_escape_unescape_roundtrip() {
        let service = MarkdownService::new();
        let original = "Patient #12345\n# Important\n---\n```code```";
        let escaped = service.escape_body(original);
        let unescaped = service.unescape_body(escaped.as_str());
        assert_eq!(unescaped.as_str(), original);
    }

    #[test]
    fn test_parse_thread_single_message_with_all_fields() {
        let service = MarkdownService::new();
        let content = "# Thread Title\n\n**Message ID:** 550e8400-e29b-41d4-a716-446655440000\n**Timestamp:** 2026-01-22T10:30:00Z\n**Author ID:** 550e8400-e29b-41d4-a716-446655440001\n**Author:** Dr Smith\n**Role:** Clinician\n\nPatient presented with symptoms.";

        let messages = service.thread_parse(content).unwrap();
        assert_eq!(messages.len(), 1);

        let msg = &messages[0];
        assert_eq!(msg.metadata.author.name.as_str(), "Dr Smith");
        assert_eq!(msg.metadata.author.role, AuthorRole::Clinician);
        assert_eq!(msg.body.as_str(), "Patient presented with symptoms.");
    }

    #[test]
    fn test_parse_thread_single_message_body_only() {
        let service = MarkdownService::new();
        let content = "**Message ID:** 550e8400-e29b-41d4-a716-446655440000\n**Timestamp:** 2026-01-22T10:30:00Z\n**Author ID:** 550e8400-e29b-41d4-a716-446655440001\n**Author:** System\n**Role:** System\n\nSimple body content.";

        let messages = service.thread_parse(content).unwrap();
        assert_eq!(messages.len(), 1);

        let msg = &messages[0];
        assert_eq!(msg.metadata.author.name.as_str(), "System");
        assert_eq!(msg.body.as_str(), "Simple body content.");
    }

    #[test]
    fn test_parse_thread_multiple_messages() {
        let service = MarkdownService::new();
        let content = "# Thread Title\n\n**Message ID:** 550e8400-e29b-41d4-a716-446655440000\n**Timestamp:** 2026-01-22T10:30:00Z\n**Author ID:** 550e8400-e29b-41d4-a716-446655440001\n**Author name:** Dr. Smith\n**Author role:** clinician\n\nFirst content\n\n---\n\n**Message ID:** 550e8400-e29b-41d4-a716-446655440002\n**Timestamp:** 2026-01-22T11:30:00Z\n**Author ID:** 550e8400-e29b-41d4-a716-446655440003\n**Author name:** Patient John\n**Author role:** patient\n\nSecond content";

        let messages = service.thread_parse(content).unwrap();
        assert_eq!(messages.len(), 2);

        assert_eq!(messages[0].body.as_str(), "First content");
        assert_eq!(messages[1].body.as_str(), "Second content");
    }

    #[test]
    fn test_parse_thread_unescapes_body() {
        let service = MarkdownService::new();
        let content = "# Thread Title\n\n**Message ID:** 550e8400-e29b-41d4-a716-446655440000\n**Timestamp:** 2026-01-22T10:30:00Z\n**Author ID:** 550e8400-e29b-41d4-a716-446655440001\n**Author name:** Dr Jones\n**Author role:** clinician\n\nPatient presented.\n\\# Important note\n\\`\\`\\`code\\`\\`\\`";

        let messages = service.thread_parse(content).unwrap();
        assert_eq!(messages.len(), 1);

        assert_eq!(
            messages[0].body.as_str(),
            "Patient presented.\n# Important note\n```code```"
        );
    }

    #[test]
    fn test_parse_thread_empty_content() {
        let service = MarkdownService::new();
        let result = service.thread_parse("");
        assert!(result.is_err());

        let result2 = service.thread_parse("   \n  \n  ");
        assert!(result2.is_err());
    }

    #[test]
    fn test_parse_thread_message_without_body_fails() {
        let service = MarkdownService::new();
        let content = "# Thread Title\n\n**Variable:** Value";

        let result = service.thread_parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_thread_roundtrip() {
        let service = MarkdownService::new();

        // Create a message
        let msg_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let author_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
        let body = "Patient #12345 update\n# Important\n```code```";
        let metadata = MessageMetadata {
            message_id: msg_id,
            timestamp: DateTime::parse_from_rfc3339("2026-01-22T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            author: MessageAuthor {
                id: author_id,
                name: NonEmptyText::new("Dr Smith").unwrap(),
                role: AuthorRole::Clinician,
            },
        };
        let msg = Message {
            metadata,
            body: NonEmptyText::new(body).unwrap(),
            corrects: None,
        };
        let created = service.thread_render(&[msg]).unwrap();

        // Parse it back
        let parsed = service.thread_parse(created.as_str()).unwrap();
        assert_eq!(parsed.len(), 1);

        let msg = &parsed[0];
        assert_eq!(msg.metadata.author.name.as_str(), "Dr Smith");
        assert_eq!(msg.metadata.author.role, AuthorRole::Clinician);
        assert_eq!(msg.body.as_str(), body);
    }

    #[test]
    fn test_parse_thread_multiple_messages_roundtrip() {
        let service = MarkdownService::new();

        // Create first message with new_thread_render
        let metadata1 = MessageMetadata {
            message_id: Uuid::nil(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-22T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            author: MessageAuthor {
                id: Uuid::nil(),
                name: NonEmptyText::new("Author 1").unwrap(),
                role: AuthorRole::Clinician,
            },
        };
        let msg1 = Message {
            metadata: metadata1,
            body: NonEmptyText::new("First message content").unwrap(),
            corrects: None,
        };

        // Create second message
        let metadata2 = MessageMetadata {
            message_id: Uuid::new_v4(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-22T11:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            author: MessageAuthor {
                id: Uuid::new_v4(),
                name: NonEmptyText::new("Author 2").unwrap(),
                role: AuthorRole::Patient,
            },
        };
        let msg2 = Message {
            metadata: metadata2,
            body: NonEmptyText::new("Second message content").unwrap(),
            corrects: None,
        };

        // Combine and render thread
        let thread = service.thread_render(&[msg1, msg2]).unwrap();

        // Parse back
        let parsed = service.thread_parse(thread.as_str()).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].body.as_str(), "First message content");
        assert_eq!(parsed[1].body.as_str(), "Second message content");
    }
}
