use prk_async_dataflow::{AsyncJsonParser, ParserConfig};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let data = r#"
        {"id": 1, "name": "Alice"}
        {"id": 2, "name": "Bob"}
        {"id": 3, "name": "Charlie"}
    "#.as_bytes();

    let mut parser = AsyncJsonParser::with_config(
        std::io::Cursor::new(data),
        ParserConfig {
            batch_size: 2,
            skip_invalid: true,
            ..Default::default()
        },
    ).into_stream::<simd_json::owned::Value>();

    while let Some(result) = parser.next().await {
        match result {
            Ok(item) => println!("Item: {:?}", item),
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}