use prk_async_dataflow::{AsyncJsonParser, DataConnector, ParserConfig, StreamToAsyncRead, WebSocketConnector};
use tokio_stream::StreamExt;
use std::io;

#[tokio::main]
async fn main() {
    // Create the WebSocket connector
    let connector = WebSocketConnector::new("ws://ws.postman-echo.com/raw").unwrap();
    let stream = connector.stream().await.unwrap();

    // Adapt the stream into an AsyncRead
    let stream = stream.map(|result| {
        result.map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    });
    let async_read = StreamToAsyncRead::new(stream);

    // Create the parser
    let mut parser = AsyncJsonParser::with_config(
        async_read,
        ParserConfig {
            batch_size: 10,
            timeout: Some(std::time::Duration::from_secs(5)),
            ..Default::default()
        },
    );

    // Process batches
    loop {
        match parser.next_batch::<simd_json::owned::Value>().await {
            Ok(batch) => {
                println!("Received batch of {} messages", batch.len());
                // Process batch
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }
}