use prk_async_dataflow::{AsyncJsonParser, DataConnector, JsonParserError, ParserConfig, StreamToAsyncRead, WebSocketConnector};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use std::{io, time::Duration};

#[derive(Deserialize, Serialize, Debug)]
struct Message {
    id: i32,
    name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the WebSocket connector
    let connector = WebSocketConnector::new("wss://ws.postman-echo.com/raw", None)?;
    
    // Connect and get the stream
    let stream = connector.stream().await?;

    // Adapt the stream into an AsyncRead
    let stream = stream.map(|result| {
        result.map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    });
    let async_read = StreamToAsyncRead::new(stream);

    // Create the parser with appropriate configuration
    let mut parser = AsyncJsonParser::with_config(
        async_read,
        ParserConfig {
            batch_size: 1,
            timeout: Some(Duration::from_secs(30)), // Increased timeout
            max_buffer_size: 1024 * 1024, // 1MB buffer
            skip_invalid: true, // Skip invalid messages
            ..Default::default()
        },
    );

    println!("Connected to WebSocket. Waiting for messages...");

    // Process messages
    loop {
        match parser.next::<Message>().await {
            Ok(msg) => {
                println!("Received message: {:?}", msg);
            }
            Err(e) => {
                match e {
                    JsonParserError::Timeout => {
                        println!("No data received within timeout period. Reconnecting...");
                        // Implement reconnection logic here if needed
                        break;
                    }
                    JsonParserError::IncompleteData => {
                        println!("Connection closed by server");
                        break;
                    }
                    _ => {
                        eprintln!("Error: {}", e);
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}