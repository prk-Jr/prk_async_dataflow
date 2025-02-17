#[cfg(test)]
mod tests {
    use crate::*;
    use serde::{Deserialize, Serialize};
    use tokio::{io::BufReader, sync::mpsc, time::sleep};
    use tokio::time::Duration;

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
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
}
