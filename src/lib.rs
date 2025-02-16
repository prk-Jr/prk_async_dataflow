//! # Streaming JSON Library
//!
//! This library provides an asynchronous streaming JSON parser that can extract and
//! deserialize JSON blocks from a stream. It can handle extra text surrounding the JSON,
//! supports both JSON objects (starting with `{`) and JSON arrays (starting with `[`),
//! and supports NDJSON (newline-delimited JSON).
//!
//! ## Example
//!
//! ```no_run
//! use tokio::sync::mpsc;
//! use tokio::time::{sleep, Duration};
//!
//! #[derive(Debug, serde::Deserialize, serde::Serialize)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a channel to simulate streaming input.
//!     let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
//!     let reader = prk_async_dataflow::ChannelReader::new(rx);
//!     let mut parser = prk_async_dataflow::AsyncJsonParser::new(reader);
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

#[cfg(test)]
mod tests;

mod reader;
pub use reader::*;

mod async_parser;
pub use async_parser::*;

mod extract_json;
pub use extract_json::*;

mod transformers;
pub use transformers::*;

mod connectors;
pub use connectors::*;

mod metrics;
pub use metrics::*;

