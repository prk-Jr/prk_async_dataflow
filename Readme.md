prk_async_dataflow
------------------

Overview:
prk_async_dataflow is an asynchronous dataflow processing library for Rust. It enables you to extract JSON and NDJSON data from streaming sources (such as network sockets, file streams, or other asynchronous inputs) without blocking the main thread. The library supports configurable options including buffer sizes, timeouts, and custom fallback parsers. Optional relaxed parsing (e.g., JSON5) is also available when the "relaxed" feature is enabled.

Features:
- Asynchronous JSON parsing built on Tokio
- Support for both standard JSON and NDJSON (newline-delimited JSON)
- Zero-copy parsing when possible, with fallback to lossy UTF-8 conversion
- Customizable configuration options:
    * Buffer size and maximum buffer size
    * Timeout for read operations
    * Custom fallback parser for alternative parsing strategies
- Optional JSON5 parsing support via the "relaxed" feature

Installation:
To add prk_async_dataflow to your project, include the following in your Cargo.toml file:
```toml
  [dependencies]
  prk_async_dataflow = "0.1.2"
```

If you require JSON5 (relaxed mode) support, enable the feature as shown:
```toml
  [dependencies]
  prk_async_dataflow = { version = "0.1.2", features = ["relaxed"] }
```
Usage Example:
The following example demonstrates how to create a simple TCP server that uses prk_async_dataflow to asynchronously parse incoming JSON messages (for instance, chat messages):

------------------------------------------------------------
```rust
use tokio::{
    io::BufReader,
    net::TcpListener,
    sync::mpsc,
};
use tokio_stream::StreamExt;
use prk_async_dataflow::AsyncJsonParser;
use serde::Deserialize;
use tracing_subscriber;

#[derive(Deserialize, Debug)]
struct ChatMessage {
    username: String,
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging (optional)
    tracing_subscriber::fmt::init();

    // Bind the TCP listener to a local address
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080");

    loop {
        // Accept incoming connections
        let (socket, addr) = listener.accept().await?;
        println!("Accepted connection from: {}", addr);

        // Spawn a new task for each connection
        tokio::spawn(async move {
            // Wrap the socket with a buffered reader
            let reader = BufReader::new(socket);
            // Create an instance of the AsyncJsonParser
            let mut parser = AsyncJsonParser::new(reader);
            // Convert the parser into an asynchronous stream of ChatMessage items
            let mut json_stream = parser.into_stream::<ChatMessage>();

            // Process each JSON message as it arrives
            while let Some(result) = json_stream.next().await {
                match result {
                    Ok(chat_msg) => {
                        println!("Received message from {}: {:?}", addr, chat_msg);
                        // Additional processing (such as broadcasting) could be added here
                    }
                    Err(err) => {
                        eprintln!("Error parsing JSON from {}: {}", addr, err);
                        // Optionally close the connection on error
                        break;
                    }
                }
            }
            println!("Connection with {} closed.", addr);
        });
    }
}
```

Similarly, we can test:
```rust
use crate::*;
    use serde::Deserialize;
    use tokio::{io::BufReader, sync::mpsc, time::sleep};
    use tokio::time::Duration;

    #[derive(Debug, Deserialize, PartialEq)]
    struct ChatMessage {
        user: String,
        text: String,
        timestamp: u64,
    }

    #[tokio::test]
    async fn test_ignore_invalid() {
        let chat_data = r#"
        {"user": "Alice", "text": "Hello!", "timestamp": 1620000000}
        {"user": "Bob", "text": "Hi Alice!", "timestamp": 1620000001}
        Invalid JSON 
        {"user": "Charlie", "text": "Hellow
        What are you doing?", "timestamp": 1620000002}
        {"user": "Charlie", "text": "How's everyone?", "timestamp": 1620000002}
    "#;

        let reader = BufReader::new(chat_data.as_bytes());
        let config = ParserConfig {
            skip_invalid: true, // Enable skipping invalid JSON
            ..Default::default()
        };
        let mut parser = AsyncJsonParser::with_config(reader, config);

        let mut messages = Vec::new();

        loop {
            match parser.next::<ChatMessage>().await {
                Ok(msg) => {
                    println!("[{}] {}: {}", msg.timestamp, msg.user, msg.text);
                    messages.push(msg);
                }
                Err(JsonParserError::IncompleteData) => break, // End of stream
                Err(e) => {
                    eprintln!("Error: {}", e);
                    break;
                }
            }
        }

        // Verify that only valid messages were parsed
        assert_eq!(messages.len(), 3);
        assert_eq!(
            messages[0],
            ChatMessage {
                user: "Alice".to_string(),
                text: "Hello!".to_string(),
                timestamp: 1620000000
            }
        );
        assert_eq!(
            messages[1],
            ChatMessage {
                user: "Bob".to_string(),
                text: "Hi Alice!".to_string(),
                timestamp: 1620000001
            }
        );
        assert_eq!(
            messages[2],
            ChatMessage {
                user: "Charlie".to_string(),
                text: "How's everyone?".to_string(),
                timestamp: 1620000002
            }
        );
    }

    #[tokio::test]
    async fn test_error_on_invalid() {
        let chat_data = r#"
        {"user": "Alice", "text": "Hello!", "timestamp": 1620000000}
        {"user": "Bob", "text": "Hi Alice!", "timestamp": 1620000001}
        Invalid JSON {"user": "Bob", "text": 
        "Hi Everyone!",
          "timestamp": 1620000001
          }
        {"user": "Charlie", "text": "How's everyone?", "timestamp": 1620000002}
    "#;

        let parts: Vec<Vec<u8>> = chat_data
            .as_bytes()
            .chunks(30)
            .map(|chunk| chunk.to_vec())
            .collect();
        let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
        let reader = ChannelReader::new(rx);
        tokio::spawn(async move {
            for part in parts {
                tx.send(part).await.unwrap();
                sleep(Duration::from_millis(10)).await;
            }
        });

        let config = ParserConfig {
            skip_invalid: true, // Disable skipping invalid JSON
            ..Default::default()
        };
        let mut parser = AsyncJsonParser::with_config(reader, config);

        let mut messages = Vec::new();

        loop {
            match parser.next::<ChatMessage>().await {
                Ok(msg) => {
                    println!("[{}] {}: {}", msg.timestamp, msg.user, msg.text);
                    messages.push(msg);
                }
                Err(JsonParserError::IncompleteData) => break, // End of stream
                Err(e) => {
                    eprintln!("Error: {}", e);
                    assert!(e.to_string().contains("Invalid data")); // Ensure error is due to invalid JSON
                    break;
                }
            }
        }

        // Verify that only valid messages before the error were parsed
        assert_eq!(messages.len(), 4);
        assert_eq!(
            messages[0],
            ChatMessage {
                user: "Alice".to_string(),
                text: "Hello!".to_string(),
                timestamp: 1620000000
            }
        );
        assert_eq!(
            messages[1],
            ChatMessage {
                user: "Bob".to_string(),
                text: "Hi Alice!".to_string(),
                timestamp: 1620000001
            }
        );
    }
```
------------------------------------------------------------

Contributing:
Contributions, bug reports, and feature suggestions are welcome. Please feel free to open issues or submit pull requests on the project's repository.

License:
This project is licensed under the MIT License.
