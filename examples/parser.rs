use std::io::Cursor;

use prk_async_dataflow::{AsyncJsonParser, ParserConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct MyData {
    id: u32,
    name: String,
}

#[tokio::main]
async fn main() {
    let data = r#"{"id": 1, "name": "Alice"}\n{"id": 2, "name": "Bob"}"#.as_bytes();
    let reader = Cursor::new(data);
    let config = ParserConfig {
        batch_size: 2,
        ..Default::default()
    };
    let mut parser = AsyncJsonParser::with_config(reader, config);

    let batch = parser.next_batch::<MyData>().await.unwrap();
    println!("Parsed batch: {:?}", batch);
}