use bytes::{Buf, BytesMut};
use memchr::memchr;
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
pub struct ParserConfig {
    pub buffer_size: usize,
    pub timeout: Option<Duration>,
    pub max_buffer_size: usize,
    pub fallback_parser:
        Option<Box<dyn Fn(&str) -> Result<serde_json::Value, JsonParserError> + Send + Sync>>,
    pub skip_invalid: bool,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1024,
            timeout: None,
            max_buffer_size: 1024 * 1024, // 1MB
            fallback_parser: None,
            skip_invalid: false,
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
        let start_len = self.buffer.len();
        self.buffer.reserve(self.config.buffer_size);

        unsafe {
            // Safety: We're using the spare capacity before setting the length
            let spare = self.buffer.spare_capacity_mut();
            let mut read_buf = tokio::io::ReadBuf::uninit(spare);

            let read_fut = self.reader.read_buf(&mut read_buf);
            let bytes_read = match self.config.timeout {
                Some(t) => timeout(t, read_fut)
                    .await
                    .map_err(|_| JsonParserError::Timeout)??,
                None => read_fut.await?,
            };

            self.buffer.set_len(start_len + bytes_read);

            if bytes_read == 0 {
                return Err(JsonParserError::IncompleteData);
            }
        }

        

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn next<T: DeserializeOwned>(&mut self) -> Result<T, JsonParserError> {
        loop {
            // Attempt zero-copy parsing if the buffer is valid UTF-8
            if let Ok(slice) = std::str::from_utf8(&self.buffer) {
                if let Some((json_str, consumed)) = extract_json(slice) {
                    debug!(
                        "Found JSON candidate: {}",
                        &json_str[..json_str.len().min(50)]
                    );

                    match serde_json::from_str::<T>(json_str) {
                        Ok(value) => {
                            self.buffer.advance(consumed);
                            return Ok(value);
                        }
                        Err(e) if e.classify() == Category::Eof => {}
                        Err(e) => {
                            // Fallback to custom parser or JSON5 if enabled
                            if let Some(fallback) = &self.config.fallback_parser {
                                let value = fallback(json_str)?;
                                self.buffer.advance(consumed);
                                return Ok(serde_json::from_value(value)?);
                            }

                            #[cfg(feature = "relaxed")]
                            {
                                if let Ok(value) = json5::from_str::<T>(json_str) {
                                    self.buffer.advance(consumed);
                                    return Ok(value);
                                }
                            }

                            warn!("JSON parsing failed: {}", e);
                            if self.config.skip_invalid {
                                warn!("Skipping invalid JSON: {}", e);
                                self.buffer.advance(consumed);
                                continue;
                            } else {
                                return Err(JsonParserError::InvalidData(format!(
                                    "JSON parsing error: {}. Input: {}...",
                                    e,
                                    &json_str[..json_str.len().min(100)]
                                )));
                            }
                        }
                    }
                }
            }

            // If the buffer is not valid UTF-8, fall back to lossy conversion
            let text = String::from_utf8_lossy(&self.buffer);
            if let Some((json_str, consumed)) = extract_json(&text) {
                debug!(
                    "Found JSON candidate (lossy): {}",
                    &json_str[..json_str.len().min(50)]
                );

                match serde_json::from_str::<T>(&json_str) {
                    Ok(value) => {
                        self.buffer.advance(consumed);
                        return Ok(value);
                    }
                    Err(e) if e.classify() == Category::Eof => {}
                    Err(e) => {
                        // Fallback to custom parser or JSON5 if enabled
                        if let Some(fallback) = &self.config.fallback_parser {
                            let value = fallback(&json_str)?;
                            self.buffer.advance(consumed);
                            return Ok(serde_json::from_value(value)?);
                        }

                        #[cfg(feature = "relaxed")]
                        {
                            if let Ok(value) = json5::from_str::<T>(&json_str) {
                                self.buffer.advance(consumed);
                                return Ok(value);
                            }
                        }

                        warn!("JSON parsing failed: {}", e);
                        if self.config.skip_invalid {
                            warn!("Skipping invalid JSON: {}", e);
                            self.buffer.advance(consumed);
                            continue;
                        } else {
                            return Err(JsonParserError::InvalidData(format!(
                                "JSON parsing error: {}. Input: {}...",
                                e,
                                &json_str[..json_str.len().min(100)]
                            )));
                        }
                    }
                }
            }

            // If no JSON is found, read more data
            self.fill_buffer().await?;
        }
    }

    #[instrument(skip(self))]
    pub async fn next_ndjson<T: DeserializeOwned>(&mut self) -> Result<T, JsonParserError> {
        loop {
            if let Some(pos) = memchr(b'\n', &self.buffer) {
                let line = self.buffer.split_to(pos + 1);
                let line = &line[..pos];

                if line.is_empty() {
                    continue;
                }

                match serde_json::from_slice(line) {
                    Ok(value) => return Ok(value),
                    Err(e) if self.config.skip_invalid => {
                        warn!("Skipping invalid NDJSON line: {}", e);
                        continue;
                    }
                    Err(e) => {
                        return Err(JsonParserError::InvalidData(format!(
                            "NDJSON error: {}. Line: {}",
                            e,
                            String::from_utf8_lossy(line)
                        )))
                    }
                }
            }

            if self.buffer.is_empty() {
                self.fill_buffer().await?;
            } else if let Ok(remaining) = serde_json::from_slice::<T>(&self.buffer) {
                let result = Ok(remaining);
                self.buffer.clear();
                return result;
            } else {
                self.fill_buffer().await?;
            }
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
