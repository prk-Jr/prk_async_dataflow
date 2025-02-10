// src/lib.rs

//! # Streaming JSON Library
//!
//! This library provides an asynchronous streaming JSON parser that can extract and
//! deserialize JSON blocks from a stream. It can handle extra text surrounding the JSON
//! and supports both JSON objects (starting with `{`) and JSON arrays (starting with `[`).
//!
//! ## Example
//!
//! ```no_run
//! use tokio::sync::mpsc;
//! use tokio::time::{sleep, Duration};
//!
//! #[derive(Debug, serde::Deserialize)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a channel to simulate streaming input.
//!     let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
//!     let reader = async_serde::ChannelReader::new(rx);
//!     let mut parser = async_serde::AsyncJsonParser::new(reader);
//!
//!     // Simulate sending a JSON response with extra text in parts.
//!     tokio::spawn(async move {
//!         let response = r#"Some header text...
//!         {
//!           "name": "Alice",
//!           "age": 30
//!         }
//!         Some footer text."#;
//!         for part in response.as_bytes().chunks(40) {
//!             tx.send(part.to_vec()).await.unwrap();
//!             sleep(Duration::from_millis(20)).await;
//!         }
//!     });
//!
//!     // Parse the next JSON object from the stream.
//!     let person: Person = parser.next().await?;
//!     println!("Parsed person: {:?}", person);
//!     Ok(())
//! }
//! ```

use serde::de::DeserializeOwned;
use serde_json::{self, error::Category};
use std::io::{Error as IoError, ErrorKind};
use tokio::io::{AsyncRead, AsyncReadExt, Error, ReadBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;


/// Extracts the first JSON block (balanced braces or brackets) from a text string.
/// It supports both JSON objects (starting with `{`) and JSON arrays (starting with `[`).
/// This function ignores braces/brackets that occur inside string literals.
///
/// Returns a tuple of the extracted JSON string and the number of bytes consumed.
pub fn extract_json_with_offset(text: &str) -> Option<(String, usize)> {
    // Trim any leading whitespace.
    let text = text.trim_start();
    // Find the first occurrence of '{' or '['.
    let start = text.find(|c: char| c == '{' || c == '[')?;
    let text_slice = &text[start..];
    let first_char = text_slice.chars().next()?;
    let (opening, closing) = match first_char {
        '{' => ('{', '}'),
        '[' => ('[', ']'),
        _ => return None, // should not occur
    };

    let mut count = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut end = None;

    // Iterate over the characters in the slice.
    for (i, c) in text_slice.char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else {
            if c == '"' {
                in_string = true;
            } else if c == opening {
                count += 1;
            } else if c == closing {
                count -= 1;
                if count == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
        }
    }
    end.map(|e| (text[start..start + e].to_string(), start + e))
}

/// A streaming JSON parser that buffers incoming data and, once available,
/// extracts a JSON block (even if extra text is present) and deserializes it.
///
/// This parser will remove only the parsed portion from its internal buffer so that
/// subsequent calls to `next()` can extract additional JSON blocks from the same stream.
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
    /// If successful, only the consumed bytes are removed from the buffer so that extra text or
    /// subsequent JSON objects remain for later parsing.
    pub async fn next<T: DeserializeOwned>(&mut self) -> Result<T, Error> {
        let mut chunk = [0u8; 1024];

        loop {
            if let Ok(text) = std::str::from_utf8(&self.buffer) {
                if let Some((json_str, consumed)) = extract_json_with_offset(text) {
                    match serde_json::from_str::<T>(&json_str) {
                        Ok(value) => {
                            // Remove only the consumed bytes from the buffer.
                            self.buffer.drain(0..consumed);
                            return Ok(value);
                        }
                        // If the error indicates incomplete JSON, we continue reading.
                        Err(e) if e.classify() == Category::Eof => {}
                        // Otherwise, if we get an error, ignore and read more.
                        Err(_) => {}
                    }
                }
            }
            let bytes_read = self.reader.read(&mut chunk).await?;
            if bytes_read == 0 {
                // No more data—attempt one final extraction.
                if let Ok(text) = std::str::from_utf8(&self.buffer) {
                    if let Some((json_str, consumed)) = extract_json_with_offset(text) {
                        return serde_json::from_str::<T>(&json_str)
                            .map(|v| {
                                self.buffer.drain(0..consumed);
                                v
                            })
                            .map_err(|e| Error::new(ErrorKind::InvalidData, e));
                    }
                }
                return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete JSON data"));
            }
            self.buffer.extend_from_slice(&chunk[..bytes_read]);
        }
    }
}

/// A custom asynchronous reader that receives data in chunks from a Tokio mpsc channel.
/// This simulates a streaming source (e.g. network data or an LLM response delivered in parts).
pub struct ChannelReader {
    rx: mpsc::Receiver<Vec<u8>>,
    buffer: Vec<u8>,
}

impl ChannelReader {
    /// Creates a new `ChannelReader` from the given mpsc receiver.
    pub fn new(rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            buffer: Vec::new(),
        }
    }
}

impl AsyncRead for ChannelReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        if self.buffer.is_empty() {
            match Pin::new(&mut self.rx).poll_recv(cx) {
                Poll::Ready(Some(chunk)) => {
                    self.buffer = chunk;
                }
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
        let remaining = buf.remaining();
        let n = std::cmp::min(remaining, self.buffer.len());
        buf.put_slice(&self.buffer[..n]);
        self.buffer.drain(0..n);
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;
    use std::io::Cursor;
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration};
    

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Person {
        name: String,
        age: u32,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Data {
        id: u32,
        name: String,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct LLMResponse {
        status: String,
        message: String,
        data: Data,
        timestamp: String,
    }

    // --- Tests for extract_json_with_offset ---

    #[test]
    fn test_extract_json_with_offset_object() {
        let text = "Header { \"key\": \"value\" } Footer";
        let (json, consumed) = extract_json_with_offset(text).unwrap();
        assert_eq!(json, "{ \"key\": \"value\" }");
        assert!(consumed > 0);
    }

    #[test]
    fn test_extract_json_with_offset_array() {
        let text = "Some text [1, 2, 3, 4] more text";
        let (json, consumed) = extract_json_with_offset(text).unwrap();
        assert_eq!(json, "[1, 2, 3, 4]");
        assert!(consumed > 0);
    }

    // --- Tests for AsyncJsonParser with pure JSON ---

    #[tokio::test]
    async fn test_async_json_parser_single_chunk() {
        let data = b"{ \"name\": \"Alice\", \"age\": 30 }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }

    #[tokio::test]
    async fn test_async_json_parser_multiple_chunks() {
        let chunks: Vec<&[u8]> = vec![b"{ \"name\": \"A", b"lice\", \"age\":", b" 30 }"];
        let stream = BufReader::new(Cursor::new(chunks.into_iter().flat_map(|s| s.to_vec()).collect::<Vec<u8>>()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }

    #[tokio::test]
    async fn test_async_json_parser_incomplete_data() {
        let data = b"{ \"name\": \"Alice\", \"age\": ";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn test_async_json_parser_invalid_data() {
        let data = b"{ \"name\": \"Alice\", \"age\": invalid }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn test_async_json_parser_delayed_stream() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);
        tokio::spawn(async move {
            tx.send(b"{ \"name\": \"A".to_vec()).await.unwrap();
            sleep(Duration::from_millis(100)).await;
            tx.send(b"lice\", \"age\": 30 }".to_vec()).await.unwrap();
        });
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }

    #[tokio::test]
    async fn test_multiple_json_objects_in_stream() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);
        tokio::spawn(async move {
            tx.send(b"{\"name\":\"Alice\", \"age\": 30}".to_vec()).await.unwrap();
            sleep(Duration::from_millis(50)).await;
            tx.send(b"{\"name\":\"Bob\", \"age\": 25}".to_vec()).await.unwrap();
        });
        let person1: Person = parser.next().await.unwrap();
        assert_eq!(person1, Person { name: "Alice".into(), age: 30 });
        let person2: Person = parser.next().await.unwrap();
        assert_eq!(person2, Person { name: "Bob".into(), age: 25 });
    }

    #[tokio::test]
    async fn test_empty_channel() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(1);
        drop(tx);
        let stream = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn test_large_json_object_spanning_chunks() {
        let expected: Vec<i32> = (0..100).collect();
        let json_str = serde_json::to_string(&expected).unwrap();
        let mid = json_str.len() / 2;
        let part1 = json_str[..mid].as_bytes().to_vec();
        let part2 = json_str[mid..].as_bytes().to_vec();
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);
        tokio::spawn(async move {
            tx.send(part1).await.unwrap();
            sleep(Duration::from_millis(50)).await;
            tx.send(part2).await.unwrap();
        });
        let result: Vec<i32> = parser.next().await.unwrap();
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_json_with_whitespace() {
        let data = b"   \n\t { \"name\": \"Alice\", \"age\": 30 }  \n ";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }

    // --- Tests for LLM response extraction (extra text around JSON) ---

    #[tokio::test]
    async fn test_llm_response_extraction() {
        let llm_response = r#"Certainly! Here's a simple JSON response example:

json
Copy
{
  "status": "success",
  "message": "Data retrieved successfully",
  "data": {
    "id": 123,
    "name": "John Doe",
    "email": "johndoe@example.com",
    "age": 30,
    "isActive": true,
    "roles": ["user", "admin"],
    "preferences": {
      "theme": "dark",
      "notifications": true
    }
  },
  "timestamp": "2023-10-05T12:34:56Z"
}
This JSON response includes a status, a message, some data about a user, and a timestamp. Let me know if you need a more specific or customized JSON response!"#;
        let parts: Vec<Vec<u8>> = llm_response
            .as_bytes()
            .chunks(30)
            .map(|chunk| chunk.to_vec())
            .collect();
        let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);
        tokio::spawn(async move {
            for part in parts {
                tx.send(part).await.unwrap();
                sleep(Duration::from_millis(10)).await;
            }
        });
        let response: LLMResponse = parser.next().await.unwrap();
        assert_eq!(response.status, "success");
        assert_eq!(response.data.name, "John Doe");
    }

    #[tokio::test]
    async fn test_async_json_parser_with_extra_text() {
        let text = r#"Some header info...
Here is the JSON:
{
  "name": "Alice",
  "age": 30
}
And here is some footer text."#;
        let parts: Vec<Vec<u8>> = text
            .as_bytes()
            .chunks(40)
            .map(|chunk| chunk.to_vec())
            .collect();
        let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);
        tokio::spawn(async move {
            for part in parts {
                tx.send(part).await.unwrap();
                sleep(Duration::from_millis(20)).await;
            }
        });
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }
}
