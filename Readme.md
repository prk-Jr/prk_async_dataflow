prk_async_dataflow
------------------

Overview:
prk_async_dataflow is an asynchronous dataflow processing library for Rust with SIMD-accelerated JSON parsing and AI agent capabilities.
It is a high-performance asynchronous dataflow processing library for Rust. It is designed to efficiently extract and process JSON and NDJSON data from various streaming sources—such as network sockets, file streams, and WebSocket connections—without blocking your application's execution. Built on Tokio, prk_async_dataflow offers a flexible API with fine-grained control over buffering, timeouts, and error handling, making it ideal for real-time data ingestion and processing tasks.

Features:
- Asynchronous JSON parsing built on Tokio
- Support for both standard JSON and NDJSON (newline-delimited JSON)
- Zero-copy parsing when possible, with fallback to lossy UTF-8 conversion
- Feature Transformation:
    * Easily apply custom transformation functions on parsed JSON objects.
- Customizable configuration options:
    * Buffer size and maximum buffer size
    * Timeout for read operations
    * Custom fallback parser for alternative parsing strategies
- Optional JSON5 parsing support via the "relaxed" feature

Installation:
To add prk_async_dataflow to your project, include the following in your Cargo.toml file:
```toml
  [dependencies]
  prk_async_dataflow = "0.2.0"
```

If you require JSON5 (relaxed mode) support, enable the feature as shown:
```toml
  [dependencies]
  prk_async_dataflow = { version = "0.2.0", features = ["relaxed"] }
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

Checkout parser example:
```rust
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


#[tokio::main]
async fn main() {
    let data = r#"{"id": 1, "name": "Alice"}\n{"id": 2, "name": "Bob"}"#.as_bytes();
    batch_parse(data).await;
    let data = r#"{"id": 1, "name": "Alice"}{"id": 2, "name": "Bob"}"#.as_bytes();
    batch_parse(data).await;
    let data = r#"
    Here is your response:
    {"id": 1, "name": "Alice"}
    Some dummy data
    {"id": 2, "name": "Bob"}"#.as_bytes();
    batch_parse(data).await;
}
```

Checkout transformer example:
```rust
use std::io::Cursor;
use prk_async_dataflow::{AsyncJsonParser, DataConnector, FeatureTransformer, HttpConnector};
use serde::{Deserialize, Serialize};
use simd_json::{base::ValueAsScalar, borrowed::Value};
use tokio_stream::StreamExt;

#[derive(Debug, Deserialize, Serialize)]
struct Post {
    id: i64,
    title: String,
    body: String,
}

#[tokio::main]
async fn main() {
    let connector = HttpConnector::new("https://jsonplaceholder.typicode.com/posts".to_string());
    let data = connector.fetch().await.unwrap();
    let reader = Cursor::new(data);
    let parser = AsyncJsonParser::new(reader);

    let mut transformer = FeatureTransformer::new();
    transformer.add_mapping("title".to_string(), Box::new(|v| {
        // Transform the title to uppercase
        if let Some(title) = v.as_str() {
            Value::String(title.to_uppercase().into())
        } else {
            v // Return the original value if it's not a string
        }
    }));

    // Parse the array of posts
    let mut stream = parser.into_stream::<Vec<Post>>();
    while let Some(result) = stream.next().await {
        match result {
            Ok(posts) => {
                for mut post in posts {
                    // Apply the transformation to the title
                    post.title = post.title.to_uppercase();
                    println!("Transformed Post: {:#?}", post);
                }
            }
            Err(e) => {
                eprintln!("Error parsing JSON: {}", e);
            }
        }
    }
}
```

Checkout Websocket example:
```rust
use futures::{SinkExt, StreamExt};
use reqwest::Url;
use simd_json::base::ValueAsScalar;
use simd_json::derived::MutableObject;
use std::collections::HashMap;
use tokio::net::TcpListener;
use tokio::task;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::protocol::Message};
use simd_json::borrowed::Value;

/// FeatureTransformer from your transformer.rs.
/// It maps a given key to a transformation function.
pub struct FeatureTransformer {
    pub mappings: HashMap<String, Box<dyn Fn(Value) -> Value + Send + Sync>>,
}

impl FeatureTransformer {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn add_mapping(&mut self, key: String, transform: Box<dyn Fn(Value) -> Value + Send + Sync>) {
        self.mappings.insert(key, transform);
    }

    pub fn transform<'a>(&self, data: Value<'a>) -> Value<'a> {
        let mut result = data.clone();
        for (key, transform) in &self.mappings {
            if let Some(value) = result.get_mut(key.as_str()) {
                *value = transform(value.clone());
            }
        }
        result
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Spawn the server as a background task.
    let server_handle = task::spawn(async {
        run_server().await.unwrap();
    });

    // Give the server a moment to start.
    sleep(Duration::from_millis(500)).await;

    // Spawn the client as a background task.
    let client_handle = task::spawn(async {
        run_client().await.unwrap();
    });

    // Wait for both tasks to finish.
    let _ = client_handle.await;
    let _ = server_handle.await;

    Ok(())
}

async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = "127.0.0.1:9001";
    let listener = TcpListener::bind(addr).await?;
    println!("Server listening on {}", addr);

    // Accept a single connection for demonstration.
    if let Ok((stream, _)) = listener.accept().await {
        println!("Server: New client connected.");
        let ws_stream = accept_async(stream).await?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Spawn a task to receive messages from the client.
        let receive_task = task::spawn(async move {
            while let Some(message) = ws_receiver.next().await {
                match message {
                    Ok(msg) => println!("Server received: {:?}", msg),
                    Err(e) => {
                        eprintln!("Server error receiving message: {:?}", e);
                        break;
                    }
                }
            }
        });

        // Spawn a task to send JSON messages (simulated posts) to the client.
        let send_task = task::spawn(async move {
            // Create a few JSON posts as &str.
            let posts = vec![
                r#"{"id": 1, "title": "hello from server", "body": "this is a post"}"#,
                r#"{"id": 2, "title": "another post", "body": "more content here"}"#,
                r#"{"id": 3, "title": "yet another post", "body": "even more content"}"#,
            ];

            for post in posts {
                println!("Server sending: {}", post);
                ws_sender.send(Message::Text(post.into())).await?;
                sleep(Duration::from_millis(500)).await;
            }
            // Attempt to close the connection gracefully.
            // Use .ok() to ignore AlreadyClosed errors.
            let _ = ws_sender.send(Message::Close(None)).await;
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        });

        // Wait for both tasks to complete.
        let _ = tokio::join!(receive_task, send_task);
        println!("Server connection closed.");
    }
    Ok(())
}

async fn run_client() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Connect to the local server.
    let url = Url::parse("ws://127.0.0.1:9001")?;
    let (ws_stream, _) = connect_async(url).await?;
    println!("Client connected to server.");

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Set up the FeatureTransformer to convert "title" to uppercase.
    let mut transformer = FeatureTransformer::new();
    transformer.add_mapping("title".to_string(), Box::new(|v: Value| {
        if let Some(title) = v.as_str() {
            Value::String(title.to_uppercase().into())
        } else {
            v
        }
    }));

    // Task to receive messages from the server and apply transformation.
    let receive_task = task::spawn(async move {
        while let Some(message) = ws_receiver.next().await {
            match message {
                Ok(msg) => {
                    match msg {
                        Message::Text( txt) => {
                            println!("Client received raw: {}", txt);
                            // Parse the JSON message using simd_json.
                            let mut bytes = txt.into_bytes();
                            let parsed: Value = simd_json::to_borrowed_value(&mut bytes).unwrap();
                            // Apply transformation.
                            let transformed = transformer.transform(parsed);
                            println!("Client transformed: {:#?}", transformed);
                        },
                        Message::Close(_) => {
                            println!("Client received close message");
                            break;
                        },
                        _ => {},
                    }
                },
                Err(e) => {
                    eprintln!("Client error receiving message: {:?}", e);
                    break;
                }
            }
        }
    });

    // Optionally, send a message to the server.
    println!("Client sending: Hello from client");
    ws_sender.send(Message::Text("Hello from client".into())).await?;

    // Wait to allow message exchange.
    sleep(Duration::from_secs(3)).await;

    // Attempt to close the connection gracefully.
    let _ = ws_sender.send(Message::Close(None)).await.ok();
    let _ = receive_task.await;
    println!("Client connection closed.");

    Ok(())
}

```
------------------------------------------------------------

Contributing:
Contributions, bug reports, and feature suggestions are welcome. Please feel free to open issues or submit pull requests on the project's repository.

License:
This project is licensed under the MIT License.
