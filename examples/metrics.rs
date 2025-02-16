use std::io::Cursor;

use prk_async_dataflow::{AsyncJsonParser, DataConnector, HttpConnector, ParserConfig};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct Post {
    id: i64,
    title: String,
    body: String,
}

#[tokio::main]

async fn main() {
    // Example usage
    let connector = HttpConnector::new("https://jsonplaceholder.typicode.com/posts").unwrap();
    let data = connector.fetch().await.unwrap();
    let reader = Cursor::new(data);
    let mut parser = AsyncJsonParser::with_config(reader, ParserConfig{
        skip_invalid: false,
        ..Default::default()
    });

    // Process some data
    while let Ok(result) = parser.next::<Post>().await {
        println!("Parsed: {:?}", result);
    }
}