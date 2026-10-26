//! Collecting a response body: in memory up to the cap, the rest appended to a temp file.

use std::sync::Arc;

use bytes::BytesMut;
use tokio::io::AsyncWriteExt as _;

use crate::{Body, NetError};

/// Receives body chunks in order.
pub(crate) struct Collector {
    cap: usize,
    memory: BytesMut,
    file: Option<(tokio::fs::File, tempfile::TempPath)>,
    len: u64,
}

impl Collector {
    pub(crate) fn new(cap: u64) -> Self {
        Self {
            cap: usize::try_from(cap).unwrap_or(usize::MAX),
            memory: BytesMut::new(),
            file: None,
            len: 0,
        }
    }

    /// Takes one chunk.
    pub(crate) async fn push(&mut self, chunk: &[u8]) -> Result<(), NetError> {
        self.len += u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        let room = self.cap.saturating_sub(self.memory.len());
        let (keep, rest) = chunk.split_at(room.min(chunk.len()));
        self.memory.extend_from_slice(keep);
        if rest.is_empty() {
            return Ok(());
        }
        if self.file.is_none() {
            let (file, path) = tempfile::NamedTempFile::new()
                .map_err(|e| NetError::spill(&e))?
                .into_parts();
            self.file = Some((tokio::fs::File::from_std(file), path));
        }
        if let Some((file, _)) = &mut self.file {
            file.write_all(rest)
                .await
                .map_err(|e| NetError::spill(&e))?;
        }
        Ok(())
    }

    /// The finished body.
    pub(crate) async fn finish(self) -> Result<Body, NetError> {
        let head = self.memory.freeze();
        match self.file {
            None => Ok(Body::Memory(head)),
            Some((mut file, path)) => {
                file.flush().await.map_err(|e| NetError::spill(&e))?;
                Ok(Body::Spilled {
                    file: Arc::new(path),
                    len: self.len,
                    head,
                })
            }
        }
    }
}
