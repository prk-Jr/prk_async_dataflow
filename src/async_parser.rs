use bytes::{Buf, BytesMut};
use serde::de::DeserializeOwned;
use serde_json::{self, error::Category};
use std::future::Future;
use std::{pin::Pin, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    time::timeout,
};
use tokio_stream::Stream;
use tracing::{debug, instrument, warn};

use crate::extract_json;

/// Custom error type for JSON parsing operations
#[derive(Debug, thiserror::Error)]
pub enum JsonParserError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Timeout while waiting for data")]
    Timeout,

    #[error("Incomplete JSON data")]
    IncompleteData,

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[cfg(feature = "relaxed")]
    #[error("JSON5 parsing error: {0}")]
    Json5(#[from] json5::Error),
}

/// Configuration for the JSON parser
#[derive(Debug, Clone)]
pub struct ParserConfig {
    pub buffer_size: usize,
    pub timeout: Option<Duration>,
    pub max_buffer_size: usize,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1024,
            timeout: None,
            max_buffer_size: 1024 * 1024, // 1MB
        }
    }
}

/// A streaming JSON parser with enhanced features
pub struct AsyncJsonParser<R> {
    reader: R,
    buffer: BytesMut,
    config: ParserConfig,
}

impl<R: AsyncRead + Unpin> AsyncJsonParser<R> {
    /// Creates a new parser with default configuration
    pub fn new(reader: R) -> Self {
        Self::with_config(reader, ParserConfig::default())
    }

    /// Creates a new parser with custom configuration
    pub fn with_config(reader: R, config: ParserConfig) -> Self {
        Self {
            reader,
            buffer: BytesMut::with_capacity(config.buffer_size),
            config,
        }
    }

    /// Converts the parser into an asynchronous stream
    pub fn into_stream<T: DeserializeOwned + std::marker::Unpin + 'static>(
        self,
    ) -> impl Stream<Item = Result<T, JsonParserError>> {
        JsonStream {
            parser: self,
            _phantom: std::marker::PhantomData,
        }
    }

    #[instrument(skip(self))]
    async fn fill_buffer(&mut self) -> Result<(), JsonParserError> {
        let mut chunk = vec![0u8; self.config.buffer_size];

        let read_fut = self.reader.read(&mut chunk);
        let bytes_read = match self.config.timeout {
            Some(t) => timeout(t, read_fut)
                .await
                .map_err(|_| JsonParserError::Timeout)??,
            None => read_fut.await?,
        };

        if bytes_read == 0 {
            debug!("Reached end of input stream");
            return Err(JsonParserError::IncompleteData);
        }

        self.buffer.extend_from_slice(&chunk[..bytes_read]);
        if self.buffer.len() > self.config.max_buffer_size {
            warn!(
                "Buffer size exceeded limit: {} > {}",
                self.buffer.len(),
                self.config.max_buffer_size
            );
            return Err(JsonParserError::InvalidData("Buffer size exceeded".into()));
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn next<T: DeserializeOwned>(&mut self) -> Result<T, JsonParserError> {
        loop {
            // Try to parse from existing buffer
            let text = String::from_utf8_lossy(&self.buffer);
            if let Some((json_str, consumed)) = extract_json(&text) {
                debug!(
                    "Found JSON candidate: {}",
                    &json_str[..json_str.len().min(50)]
                );

                match serde_json::from_str::<T>(&json_str) {
                    Ok(value) => {
                        self.buffer.advance(consumed);
                        return Ok(value);
                    }
                    Err(e) if e.classify() == Category::Eof => {}
                    Err(e) => {
                        #[cfg(feature = "relaxed")]
                        {
                            if let Ok(value) = json5::from_str::<T>(&json_str) {
                                self.buffer.advance(consumed);
                                return Ok(value);
                            }
                        }
                        warn!("JSON parsing failed: {}", e);
                        return Err(JsonParserError::InvalidData(format!(
                            "JSON parsing error: {}. Input: {}...",
                            e,
                            &json_str[..json_str.len().min(100)]
                        )));
                    }
                }
            }

            // If we didn't find anything, read more data
            self.fill_buffer().await?;
        }
    }

    #[instrument(skip(self))]
    pub async fn next_ndjson<T: DeserializeOwned>(&mut self) -> Result<T, JsonParserError> {
        loop {
            // Check if there's a newline in the buffer
            if let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
                let line = self.buffer.split_to(pos + 1);
                let line_str = String::from_utf8_lossy(&line);
                let trimmed = line_str.trim();

                if trimmed.is_empty() {
                    continue;
                }

                return serde_json::from_str(trimmed).map_err(|e| {
                    JsonParserError::InvalidData(format!("NDJSON error: {}. Line: {}", e, trimmed))
                });
            }

            // If no newline, check if we've reached the end of the stream
            if self.buffer.is_empty() {
                return Err(JsonParserError::IncompleteData);
            }

            // If no newline but there's data, try parsing the remaining buffer as a single JSON object
            let remaining = String::from_utf8_lossy(&self.buffer);
            let trimmed = remaining.trim();

            if !trimmed.is_empty() {
                let result = serde_json::from_str::<T>(trimmed).map_err(|e| {
                    JsonParserError::InvalidData(format!("NDJSON error: {}. Line: {}", e, trimmed))
                });

                if result.is_ok() {
                    self.buffer.clear();
                }

                return result;
            }

            // If we didn't find anything, read more data
            self.fill_buffer().await?;
        }
    }
}

/// Async stream implementation
struct JsonStream<R, T> {
    parser: AsyncJsonParser<R>,
    _phantom: std::marker::PhantomData<T>,
}

impl<R: AsyncRead + Unpin, T: DeserializeOwned + std::marker::Unpin> Stream for JsonStream<R, T> {
    type Item = Result<T, JsonParserError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let fut = self.as_mut().get_mut().parser.next::<T>();
        tokio::pin!(fut);

        match fut.poll(cx) {
            std::task::Poll::Ready(Ok(item)) => std::task::Poll::Ready(Some(Ok(item))),
            std::task::Poll::Ready(Err(JsonParserError::IncompleteData)) => {
                std::task::Poll::Ready(None)
            }
            std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Some(Err(e))),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}
