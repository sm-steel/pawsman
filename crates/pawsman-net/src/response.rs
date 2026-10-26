//! A finished response.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;

/// A response body: in memory up to the cap, the rest in a temp file.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Body {
    /// The whole body.
    Memory(Bytes),
    /// The first `cap` bytes in `head`, the rest in `file`; `len` counts both.
    Spilled {
        /// The bytes after the cap; deleted when the last clone is dropped.
        file: Arc<tempfile::TempPath>,
        /// The whole length.
        len: u64,
        /// The bytes kept in memory.
        head: Bytes,
    },
}

impl Body {
    /// The whole length in bytes.
    #[must_use]
    pub fn len(&self) -> u64 {
        match self {
            Self::Memory(bytes) => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            Self::Spilled { len, .. } => *len,
        }
    }

    /// Whether the body is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The bytes held in memory.
    #[must_use]
    pub fn preview(&self) -> &[u8] {
        match self {
            Self::Memory(bytes) | Self::Spilled { head: bytes, .. } => bytes,
        }
    }
}

/// Status, headers, body and timing of a finished request.
#[derive(Debug, Clone)]
pub struct Response {
    pub(crate) status: u16,
    pub(crate) reason: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Body,
    pub(crate) ttfb: Duration,
    pub(crate) elapsed: Duration,
}

impl Response {
    /// The status code.
    #[must_use]
    pub fn status(&self) -> u16 {
        self.status
    }

    /// The canonical reason phrase, or empty.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Header names (lowercase) and values, in arrival order.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// The body.
    #[must_use]
    pub fn body(&self) -> &Body {
        &self.body
    }

    /// From sending to the response head.
    #[must_use]
    pub fn ttfb(&self) -> Duration {
        self.ttfb
    }

    /// From sending to the last body byte.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// A response for tests in other crates.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn for_tests(status: u16, body: &[u8], elapsed: Duration) -> Self {
        Self {
            status,
            reason: http::StatusCode::from_u16(status)
                .ok()
                .and_then(|s| s.canonical_reason())
                .unwrap_or("")
                .to_owned(),
            headers: Vec::new(),
            body: Body::Memory(Bytes::copy_from_slice(body)),
            ttfb: elapsed,
            elapsed,
        }
    }
}
