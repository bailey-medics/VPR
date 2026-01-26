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
