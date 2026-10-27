//! The trace of an interpolation: where each variable's value came from.

use std::fmt;
use std::ops::Range;

use crate::scope::Source;

/// Where a variable was used.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Field {
    /// The URL.
    Url,
    /// A header's name.
    HeaderName(String),
    /// A header's value.
    HeaderValue(String),
    /// A query parameter's key.
    QueryKey(String),
    /// A query parameter's value.
    QueryValue(String),
    /// The body.
    Body,
    /// The value of another variable, for nested uses.
    Var(String),
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url => f.write_str("url"),
            Self::HeaderName(name) | Self::HeaderValue(name) => write!(f, "headers.{name}"),
            Self::QueryKey(key) | Self::QueryValue(key) => write!(f, "query.{key}"),
            Self::Body => f.write_str("body"),
            Self::Var(name) => write!(f, "vars.{name}"),
        }
    }
}

/// A traced value, never a secret's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceValue {
    /// The value as resolved.
    Plain(String),
    /// A secret, or a value holding one: never shown.
    Secret,
}

/// One variable use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEntry {
    pub(crate) field: Field,
    pub(crate) span: Range<usize>,
    pub(crate) name: String,
    pub(crate) source: Source,
    pub(crate) value: TraceValue,
}

impl TraceEntry {
    /// Where the variable was used.
    #[must_use]
    pub fn field(&self) -> &Field {
        &self.field
    }

    /// Its `{{…}}` in that field's template text.
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    /// The name as written: `base`, `env.HOME`, `$uuid`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The scope the value came from.
    #[must_use]
    pub fn source(&self) -> &Source {
        &self.source
    }

    /// The value, or [`TraceValue::Secret`].
    #[must_use]
    pub fn value(&self) -> &TraceValue {
        &self.value
    }
}

/// Every variable use of one interpolation, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trace {
    pub(crate) entries: Vec<TraceEntry>,
}

impl Trace {
    /// The uses in order; nested uses follow the use that needed them.
    #[must_use]
    pub fn entries(&self) -> &[TraceEntry] {
        &self.entries
    }
}
