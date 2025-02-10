use serde::de::DeserializeOwned;
use serde_json::{self, error::Category};
use std::io::ErrorKind;
use tokio::io::{AsyncRead, AsyncReadExt, Error};

use crate::extract_json;
/// A streaming JSON parser that buffers incoming data and, once available,
/// extracts a JSON block and deserializes it. It leaves any leftover data
/// in the buffer so that subsequent calls to `next()` can parse additional JSON blocks.
pub struct AsyncJsonParser<R> {
    reader: R,
    buffer: Vec<u8>,
}

impl<R: AsyncRead + Unpin> AsyncJsonParser<R> {
    /// Creates a new JSON parser from the given asynchronous reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
        }
    }

    /// Reads data into an internal buffer until a valid JSON block can be extracted
    /// (using `extract_json_with_offset`), then attempts to deserialize that block into type `T`.
    ///
    /// If successful, only the consumed bytes are removed from the buffer.
    /// If parsing fails, the error message includes context.
    pub async fn next<T: DeserializeOwned>(&mut self) -> Result<T, Error> {
        let mut chunk = [0u8; 1024];
        loop {
            {
                // Use from_utf8_lossy to handle invalid UTF-8 bytes.
                let text = String::from_utf8_lossy(&self.buffer);
                if let Some((json_str, consumed)) = extract_json(&text) {
                    match serde_json::from_str::<T>(&json_str) {
                        Ok(value) => {
                            self.buffer.drain(0..consumed);
                            return Ok(value);
                        }
                        Err(e) if e.classify() == Category::Eof => {}
                        Err(e) => {
                            #[cfg(feature = "relaxed")]
                            {
                                if let Ok(value) = json5::from_str::<T>(&json_str) {
                                    self.buffer.drain(0..consumed);
                                    return Ok(value);
                                }
                            }
                            return Err(Error::new(
                                ErrorKind::InvalidData,
                                format!(
                                    "JSON parsing error: {}. Input snippet: {}",
                                    e,
                                    &json_str[..std::cmp::min(100, json_str.len())]
                                ),
                            ));
                        }
                    }
                }
            } // end block: drop borrow of self.buffer

            let bytes_read = self.reader.read(&mut chunk).await?;
            if bytes_read == 0 {
                {
                    let text = String::from_utf8_lossy(&self.buffer);
                    if let Some((json_str, consumed)) = extract_json(&text) {
                        return serde_json::from_str::<T>(&json_str)
                            .map(|v| {
                                self.buffer.drain(0..consumed);
                                v
                            })
                            .map_err(|e| {
                                Error::new(
                                    ErrorKind::InvalidData,
                                    format!(
                                        "Final JSON parse error: {}. Input snippet: {}",
                                        e,
                                        &json_str[..std::cmp::min(100, json_str.len())]
                                    ),
                                )
                            });
                    }
                }
                return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete JSON data"));
            }
            self.buffer.extend_from_slice(&chunk[..bytes_read]);
        }
    }

    /// Reads the next NDJSON (newline-delimited JSON) line from the stream and parses it.
    pub async fn next_ndjson<T: DeserializeOwned>(&mut self) -> Result<T, Error> {
        loop {
            if let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
                let line_bytes = self.buffer.drain(0..=pos).collect::<Vec<u8>>();
                let line = String::from_utf8_lossy(&line_bytes);
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                return serde_json::from_str::<T>(trimmed).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("NDJSON parse error: {}. Input: {}", e, trimmed),
                    )
                });
            }
            let mut chunk = [0u8; 1024];
            let bytes_read = self.reader.read(&mut chunk).await?;
            if bytes_read == 0 {
                if self.buffer.is_empty() {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "No more data"));
                }
                let line = {
                    let line = String::from_utf8_lossy(&self.buffer);
                    line.to_string()
                };
                self.buffer.clear();
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "No JSON found"));
                }
                return serde_json::from_str::<T>(trimmed).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("NDJSON final parse error: {}. Input: {}", e, trimmed),
                    )
                });
            }
            self.buffer.extend_from_slice(&chunk[..bytes_read]);
        }
    }
}

