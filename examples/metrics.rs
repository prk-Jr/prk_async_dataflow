use std::io::Cursor;
use prk_async_dataflow::{AsyncJsonParser, DataConnector, HttpConnector, ParserConfig};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
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
    let reader = Cursor::new(data.clone());
    let mut parser = AsyncJsonParser::with_config(reader, ParserConfig {
        skip_invalid: false,
        ..Default::default()
    });

    // Process single items
    while let Ok(result) = parser.next_batch_array::<Vec<Post>>().await {
        println!("Parsed: {:?}", result);
    }

    let reader = Cursor::new(data.clone());
    let mut parser = AsyncJsonParser::with_config(reader, ParserConfig {
        skip_invalid: false,
        ..Default::default()
    });

    // Process single items
    while let Ok(result) = parser.next::<Vec<Post>>().await {
        println!("Parsed as  array: {:?}", result);
    }

    // Process arrays
    let reader = Cursor::new(data.clone());
    let mut parser = AsyncJsonParser::with_config(reader, ParserConfig {
        skip_invalid: false,
        ..Default::default()
    }).into_stream::<Post>();

    while let Some(Ok(result)) = parser.next().await {
        println!("Parsed as Stream: {:?}", result);
    }

    // Process arrays
    let reader = Cursor::new(data);
    let mut parser = AsyncJsonParser::with_config(reader, ParserConfig {
        skip_invalid: false,
        ..Default::default()
    }).into_stream::<Vec<Post>>();

    while let Some(Ok(result)) = parser.next().await {
        println!("Parsed as Stream vec: {:?}", result);
    }
}