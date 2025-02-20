use bytes::{Buf, BytesMut};
#[cfg(feature = "metrics")]
use lazy_static::lazy_static;

#[cfg(feature = "metrics")]
use prometheus::{IntCounter, IntGauge, register_int_counter, register_int_gauge};
use serde::de::DeserializeOwned;
use serde::Serialize;
use simd_json::{Error as SimdJsonError, OwnedValue};
use std::{pin::Pin, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    time::timeout,
};
use tokio_stream::Stream;
use tracing::{debug, instrument, warn};
use futures::Future;

use crate::{extract_json, FeatureTransformer};

#[cfg(feature = "metrics")]
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
        Box<dyn Fn(&str) -> Result<OwnedValue, JsonParserError> + Send + Sync>,
    >,
    pub skip_invalid: bool,
    pub batch_size: usize,
    pub validation_schema: Option<schemars::schema::RootSchema>,
    pub transformer: Option<Arc<FeatureTransformer>>,

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
            transformer: None,
        }
    }
}

pub struct AsyncJsonParser<R> {
    reader: R,
    buffer: BytesMut,
    config: ParserConfig,
    pending: Vec<OwnedValue>,
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
            pending: Vec::new(),
        }
    }

    pub fn into_stream<T: DeserializeOwned + Unpin + 'static + Serialize>(
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
        #[cfg(feature = "metrics")]
        BUFFER_SIZE_GAUGE.set(self.buffer.len() as i64);
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn next<T: DeserializeOwned + Serialize>(&mut self) -> Result<T, JsonParserError> {
        loop {
            // Process pending items first
            while let Some(value) = self.pending.pop() {
                match simd_json::serde::from_owned_value(value) {
                    Ok(t) => return Ok(t),
                    Err(e) => {
                        if self.config.skip_invalid {
                            warn!("Skipping invalid JSON element: {}", e);
                            continue;
                        } else {
                            return Err(JsonParserError::Json(e));
                        }
                    }
                }
            }

            if let Some((json_bytes, consumed)) = extract_json(&self.buffer.clone()) {
                debug!(
                    "Found JSON candidate: {:?}",
                    String::from_utf8_lossy(&json_bytes[..json_bytes.len().min(50)])
                );

                let result = self.parse_json(json_bytes).await;
                self.buffer.advance(consumed);
                
                match result {
                    Ok(ValueOrArray::Value(value)) => return Ok(value),
                    Ok(ValueOrArray::Array(array)) => {
                        // Reverse to maintain order when popping
                        self.pending = array.into_iter().rev().collect();
                        continue;
                    }
                    Err(e) => {
                        if self.config.skip_invalid {
                            warn!("Skipping invalid JSON: {}", e);
                            continue;
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
            self.fill_buffer().await?;
        }
    }

    async fn parse_json<T: DeserializeOwned + Serialize>(
        &mut self,
        json_bytes: &[u8],
    ) -> Result<ValueOrArray<T>, JsonParserError> {
        let mut buffer_copy = json_bytes.to_vec();
        
        // First attempt SIMD parsing
        match simd_json::from_slice(&mut buffer_copy) {
            Ok(simd_json::OwnedValue::Array(array)) => {
                if let Some(transformer) = &self.config.transformer {
                    let transformed = array.into_iter()
                        .map(|v| transformer.transform(v))
                        .collect();
                    Ok(ValueOrArray::Array(transformed))
                } else {
                    Ok(ValueOrArray::Array(array))
                }
            }
            Ok(mut value) => {
                if let Some(transformer) = &self.config.transformer {
                    value = transformer.transform(value);
                }
                simd_json::serde::from_owned_value(value).map(ValueOrArray::Value).map_err(Into::into)
            }
            Err(_) => {
                #[cfg(feature = "relaxed")]
                {
                    let mut json_str = String::from_utf8_lossy(json_bytes).into_owned();
                    // Wrap in braces if not already an object/array
                    if !json_str.starts_with('{') && !json_str.starts_with('[') {
                        json_str = format!("{{{json_str}}}");
                    }
                    match json5::from_str::<T>(&json_str) {
                        Ok(value) => {
                            let mut json_bytes = simd_json::to_vec(&value).map_err(|e| JsonParserError::Json(e))?;
                            let mut owned_value = simd_json::to_owned_value(&mut json_bytes).map_err(|e| JsonParserError::Json(e))?;
                            if let Some(transformer) = &self.config.transformer {
                                owned_value = transformer.transform(owned_value);
                            }
                            simd_json::serde::from_owned_value(owned_value)
                                .map(ValueOrArray::Value)
                                .map_err(|e| JsonParserError::Json(e))
                        }
                        Err(e) => Err(JsonParserError::Json5(e)),
                    }
                }
                #[cfg(not(feature = "relaxed"))]
                {
                    Err(JsonParserError::InvalidData(format!(
                        "Failed to parse JSON: {}",
                        String::from_utf8_lossy(json_bytes)
                    )))
                }
            }
        }
    }
    
    

    #[instrument(skip(self))]
    pub async fn next_batch<T: DeserializeOwned + Serialize>(
        &mut self,
    ) -> Result<Vec<T>, JsonParserError> {
        let mut batch = Vec::with_capacity(self.config.batch_size);
        while batch.len() < self.config.batch_size {
            match self.next::<T>().await {
                Ok(item) => batch.push(item),
                Err(JsonParserError::IncompleteData) => break,
                Err(e) => return Err(e),
            }
        }
        
        #[cfg(feature = "metrics")]
        if !batch.is_empty() {
            PARSED_JSON_COUNT.inc_by(batch.len() as u64);
        }
        Ok(batch)
    }

    #[instrument(skip(self))]
    pub async fn next_batch_array<T: DeserializeOwned + Serialize>(
        &mut self,
    ) -> Result<Vec<T>, JsonParserError> {
        let mut batch = Vec::with_capacity(self.config.batch_size);
        while batch.len() < self.config.batch_size {
            match self.next::<Vec<T>>().await {
                Ok(items) => batch.extend(items),
                Err(JsonParserError::IncompleteData) => break,
                Err(e) => return Err(e),
            }
        }
        #[cfg(feature = "metrics")]
        if !batch.is_empty() {
            PARSED_JSON_COUNT.inc_by(batch.len() as u64);
        }
        Ok(batch)
    }
}

enum ValueOrArray<T> {
    Value(T),
    Array(Vec<simd_json::OwnedValue>),
}

struct JsonStream<R, T> {
    parser: AsyncJsonParser<R>,
    _phantom: std::marker::PhantomData<T>,
}

impl<R: AsyncRead + Unpin, T: DeserializeOwned + Unpin + Serialize> Stream for JsonStream<R, T> {
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