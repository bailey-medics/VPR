/// Errors that can occur when creating validated text types.
#[derive(Debug, thiserror::Error)]
pub enum TextError {
    /// The input text was empty or contained only whitespace
    #[error("Text cannot be empty")]
    Empty,
}

/// A string type that guarantees non-empty content.
///
/// This type wraps a `String` and ensures it contains at least one non-whitespace character.
/// The input is automatically trimmed of leading and trailing whitespace during construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonEmptyText(String);

impl NonEmptyText {
    /// Creates a new `NonEmptyText` from the given input.
    ///
    /// The input is trimmed of leading and trailing whitespace. If the trimmed
    /// result is empty, an error is returned.
    ///
    /// # Arguments
    ///
    /// * `input` - Any type that can be converted to a string reference
    ///
    /// # Returns
    ///
    /// Returns `Ok(NonEmptyText)` if the trimmed input is non-empty,
    /// or `Err(TextError::Empty)` if it's empty or contains only whitespace.
    pub fn new(input: impl AsRef<str>) -> Result<Self, TextError> {
        let trimmed = input.as_ref().trim();
        if trimmed.is_empty() {
            return Err(TextError::Empty);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Returns the inner string as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NonEmptyText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for NonEmptyText {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl serde::Serialize for NonEmptyText {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for NonEmptyText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NonEmptyText::new(&s).map_err(serde::de::Error::custom)
    }
}

use std::fmt;
use std::str::FromStr;

/// A syntactically plausible email address.
///
/// Guarantees:
/// - non-empty
/// - exactly one '@'
/// - non-empty local and domain parts
/// - no surrounding whitespace
/// - length is within reasonable bounds
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmailAddress(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailAddressError {
    Empty,
    ContainsWhitespace,
    MissingAtSymbol,
    MultipleAtSymbols,
    EmptyLocalPart,
    EmptyDomainPart,
    TooLong,
}

impl EmailAddress {
    /// Maximum length chosen to be conservative and practical.
    const MAX_LENGTH: usize = 254;

    pub fn parse(input: &str) -> Result<Self, EmailAddressError> {
        let value = input.trim();

        if value.is_empty() {
            return Err(EmailAddressError::Empty);
        }

        if value.len() > Self::MAX_LENGTH {
            return Err(EmailAddressError::TooLong);
        }

        if value.chars().any(char::is_whitespace) {
            return Err(EmailAddressError::ContainsWhitespace);
        }

        let at_count = value.matches('@').count();

        match at_count {
            0 => return Err(EmailAddressError::MissingAtSymbol),
            1 => {}
            _ => return Err(EmailAddressError::MultipleAtSymbols),
        }

        let (local, domain) = value
            .split_once('@')
            .expect("exactly one '@' already verified");

        if local.is_empty() {
            return Err(EmailAddressError::EmptyLocalPart);
        }

        if domain.is_empty() {
            return Err(EmailAddressError::EmptyDomainPart);
        }

        Ok(Self(value.to_owned()))
    }

    /// Borrow the email address as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for EmailAddress {
    type Err = EmailAddressError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        EmailAddress::parse(s)
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for EmailAddress {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
