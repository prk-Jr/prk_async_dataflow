use std::io::Cursor;

use prk_async_dataflow::{AsyncJsonParser, ParserConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct MyData {
    id: u32,
    name: String,
}

async fn batch_parse(data: &[u8]) {
    let reader = Cursor::new(data);
    let config = ParserConfig {
        batch_size: 2,
        ..Default::default()
    };
    let mut parser = AsyncJsonParser::with_config(reader, config);

    let batch = parser.next_batch::<MyData>().await.unwrap();
    println!("Parsed batch: {:?}", batch);
}



// To Run: cargo run --features relaxed --example parser
#[tokio::main]
async fn main() {
    let data = r#"{"id": 1, "name": "Alice"}\n{"id": 2, "name": "Bob"}"#.as_bytes();
    batch_parse(data).await;
    let data = r#"{"id": 2, "name": "Charlie"}{"id": 2, "name": "Bob"}"#.as_bytes();
    batch_parse(data).await;
    let data = r#"
    Here is your response:
    {"id": 1, "name": "Alice"}
    Some dummy data
    {"id": 2, "name": "Bob"}"#.as_bytes();
    batch_parse(data).await;
    let data = r#"
    b"Here is your response:{name:'Prakash', id:30, extra:'remove me', yap:'   noisy message   '}"
"#.as_bytes();
    batch_parse(data).await;
}