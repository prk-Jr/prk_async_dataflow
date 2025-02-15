use futures::{SinkExt, StreamExt};
use prk_async_dataflow::FeatureTransformer;
use reqwest::Url;
use simd_json::base::ValueAsScalar;
use tokio::net::TcpListener;
use tokio::task;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::protocol::Message};
use simd_json::borrowed::Value;


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
