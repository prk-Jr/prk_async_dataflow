use bytes::{Buf, BytesMut};
use lazy_static::lazy_static;
use memchr::memchr;
use prometheus::{IntCounter, IntGauge, register_int_counter, register_int_gauge};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use simd_json::{Error as SimdJsonError, OwnedValue};
use std::future::Future;
use std::{pin::Pin, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    time::timeout,
};
use tokio_stream::Stream;
use tracing::{debug, instrument, warn};
use validator::Validate;

use crate::extract_json;

// Initialize Prometheus metrics
lazy_static! {
    static ref PARSED_JSON_COUNT: IntCounter = register_int_counter!(
        "parsed_json_count",
        "Total number of JSON documents parsed"
    ).unwrap();
    static ref BUFFER_SIZE_GAUGE: IntGauge = register_int_gauge!(
        "buffer_size_bytes",
        "Current size of the parser buffer in bytes"
    ).unwrap();
}

#[derive(Debug, thiserror::Error)]
pub enum JsonParserError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] SimdJsonError),
    #[error("Timeout while waiting for data")]
    Timeout,
    #[error("Incomplete JSON data")]
    IncompleteData,
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("Schema validation failed: {0}")]
    ValidationError(String),
    #[cfg(feature = "relaxed")]
    #[error("JSON5 parsing error: {0}")]
    Json5(#[from] json5::Error),
}

pub struct ParserConfig {
    pub buffer_size: usize,
    pub timeout: Option<Duration>,
    pub max_buffer_size: usize,
    pub fallback_parser: Option<
        Box<dyn Fn(&str) -> Result<simd_json::OwnedValue, JsonParserError> + Send + Sync>,
    >,
    pub skip_invalid: bool,
    pub batch_size: usize,
    pub validation_schema: Option<schemars::schema::RootSchema>,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1024,
            timeout: None,
            max_buffer_size: 1024 * 1024,
            fallback_parser: None,
            skip_invalid: false,
            batch_size: 10,
            validation_schema: None,
        }
    }
}

pub struct AsyncJsonParser<R> {
    reader: R,
    buffer: BytesMut,
    config: ParserConfig,
}

impl<R: AsyncRead + Unpin> AsyncJsonParser<R> {
    pub fn new(reader: R) -> Self {
        Self::with_config(reader, ParserConfig::default())
    }

    pub fn with_config(reader: R, config: ParserConfig) -> Self {
        Self {
            reader,
            buffer: BytesMut::with_capacity(config.buffer_size),
            config,
        }
    }

    pub fn into_stream<T: DeserializeOwned + Unpin + 'static>(
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

        BUFFER_SIZE_GAUGE.set(self.buffer.len() as i64);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn next<T: DeserializeOwned>(
        &mut self,
    ) -> Result<T, JsonParserError> {
        loop {
            if let Some((json_bytes, consumed)) = extract_json(&self.buffer) {
                debug!("Found JSON candidate: {:?}", &json_bytes[..json_bytes.len().min(50)]);

                let mut buffer_copy = json_bytes.to_vec();
                match simd_json::from_slice(&mut buffer_copy) {
                    Ok(value) => {
                        self.buffer.advance(consumed);
                        return Ok(value);
                    }
                    Err(e) => {
                        let json_str = String::from_utf8_lossy(json_bytes);
                        if let Some(fallback) = &self.config.fallback_parser {
                            let value = fallback(&json_str)?;
                            self.buffer.advance(consumed);
                            let owned_value: simd_json::OwnedValue = value;
                            let mut json_str = owned_value.to_string();
                            let deserialized_value: T = unsafe {
                                simd_json::from_str(&mut json_str)? 
                            }; 
                            return Ok(deserialized_value);
                        }
                        #[cfg(feature = "relaxed")]
                        {
                            if let Ok(value) = json5::from_str(&json_str) {
                                self.buffer.advance(consumed);
                                return Ok(value);
                            }
                        }
                        if self.config.skip_invalid {
                            warn!("Skipping invalid JSON: {}", e);
                            self.buffer.advance(consumed);
                            continue;
                        }
                        return Err(JsonParserError::InvalidData(format!(
                            "JSON error: {}. Input: {}...",
                            e,
                            &json_str[..json_str.len().min(100)]
                        )));
                    }
                }
            }
            self.fill_buffer().await?;
        }
    }

    #[instrument(skip(self))]
    pub async fn next_batch<T: DeserializeOwned>(
        &mut self,
    ) -> Result<Vec<T>, JsonParserError> {
        let mut batch = Vec::with_capacity(self.config.batch_size);
        while batch.len() < self.config.batch_size {
            match self.next::<T>().await {
                Ok(item) => {
                    batch.push(item);
                }
                Err(JsonParserError::IncompleteData) => break,
                Err(e) => return Err(e),
            }
        }
        PARSED_JSON_COUNT.inc_by(batch.len() as u64);
        Ok(batch)
    }
}

struct JsonStream<R, T> {
    parser: AsyncJsonParser<R>,
    _phantom: std::marker::PhantomData<T>,
}

impl<R: AsyncRead + Unpin, T: DeserializeOwned + Unpin> Stream for JsonStream<R, T> {
    type Item = Result<T, JsonParserError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let fut = self.parser.next::<T>();
        tokio::pin!(fut);
        match fut.poll(cx) {
            std::task::Poll::Ready(Ok(item)) => std::task::Poll::Ready(Some(Ok(item))),
            std::task::Poll::Ready(Err(JsonParserError::IncompleteData)) => std::task::Poll::Ready(None),
            std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Some(Err(e))),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}