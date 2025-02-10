#[cfg(test)]
mod tests {
    use crate::{extract_json, AsyncJsonParser, ChannelReader};
    use tokio::io::BufReader;
    use std::io::Cursor;
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration};

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Person {
        name: String,
        age: u32,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct LogEntry {
        level: String,
        message: String,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Data {
        id: u32,
        name: String,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct LLMResponse {
        status: String,
        message: String,
        data: Data,
        timestamp: String,
    }

    // --- Tests for extract_json ---
    #[test]
    fn test_extract_json_object() {
        let text = "Header { \"key\": \"value\" } Footer";
        let (json, consumed) = extract_json(text).unwrap();
        assert_eq!(json, "{ \"key\": \"value\" }");
        assert!(consumed > 0);
    }

    #[test]
    fn test_extract_json_array() {
        let text = "Some text [1, 2, 3, 4] more text";
        let (json, consumed) = extract_json(text).unwrap();
        assert_eq!(json, "[1, 2, 3, 4]");
        assert!(consumed > 0);
    }

    // --- Tests for AsyncJsonParser (strict mode) ---
    #[tokio::test]
    async fn test_async_json_parser_single_chunk() {
        let data = b"{ \"name\": \"Alice\", \"age\": 30 }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }

    #[tokio::test]
    async fn test_multiple_json_blocks_in_stream() {
        // Two JSON objects in one stream with extra text in between.
        let data = r#"{"name": "Alice", "age": 30} some extra text {"name": "Bob", "age": 25}"#;
        let stream = BufReader::new(Cursor::new(data.as_bytes().to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person1: Person = parser.next().await.unwrap();
        assert_eq!(person1, Person { name: "Alice".into(), age: 30 });
        let person2: Person = parser.next().await.unwrap();
        assert_eq!(person2, Person { name: "Bob".into(), age: 25 });
    }

    // --- Tests for NDJSON ---
    #[tokio::test]
    async fn test_next_ndjson_single_line() {
        let ndjson = r#"{"level": "INFO", "message": "Hello NDJSON"}"#;
        let stream = BufReader::new(Cursor::new(ndjson.as_bytes().to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let entry: LogEntry = parser.next_ndjson().await.unwrap();
        assert_eq!(
            entry,
            LogEntry {
                level: "INFO".into(),
                message: "Hello NDJSON".into()
            }
        );
    }

    #[tokio::test]
    async fn test_next_ndjson_multiple_lines() {
        let ndjson = r#"{"level": "INFO", "message": "First"}
{"level": "WARN", "message": "Second"}
{"level": "ERROR", "message": "Third"}"#;
        let stream = BufReader::new(Cursor::new(ndjson.as_bytes().to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let entry1: LogEntry = parser.next_ndjson().await.unwrap();
        let entry2: LogEntry = parser.next_ndjson().await.unwrap();
        let entry3: LogEntry = parser.next_ndjson().await.unwrap();
        assert_eq!(
            entry1,
            LogEntry {
                level: "INFO".into(),
                message: "First".into()
            }
        );
        assert_eq!(
            entry2,
            LogEntry {
                level: "WARN".into(),
                message: "Second".into()
            }
        );
        assert_eq!(
            entry3,
            LogEntry {
                level: "ERROR".into(),
                message: "Third".into()
            }
        );
    }

    // --- Tests for Improved Error Reporting ---
    #[tokio::test]
    async fn test_error_reporting_invalid_json() {
        let data = b"{ \"name\": \"Alice\", \"age\": }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("JSON parsing error"));
    }

    // --- Tests for Robust Handling of Encoding Issues ---
    #[tokio::test]
    async fn test_encoding_issue_handling() {
        // Append an invalid byte (0xFF) to valid UTF-8 JSON.
        let mut data = b"{ \"name\": \"Alice\", \"age\": 30 }".to_vec();
        data.push(0xFF);
        let stream = BufReader::new(Cursor::new(data));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }

    // --- Tests for Relaxed JSON Formats ---
    // Enable the "relaxed" feature in Cargo.toml and add `json5` as an optional dependency.
    #[cfg(feature = "relaxed")]
    #[tokio::test]
    async fn test_relaxed_json_parsing() {
        let data = b"{ \"name\": \"Alice\", \"age\": 30, }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }

    // --- Tests for LLM Response Extraction (Extra Text) ---
    #[tokio::test]
    async fn test_llm_response_extraction() {
        let llm_response = r#"Certainly! Here's a simple JSON response example:

json
Copy
{
  "status": "success",
  "message": "Data retrieved successfully",
  "data": {
    "id": 123,
    "name": "John Doe"
  },
  "timestamp": "2023-10-05T12:34:56Z"
}
This JSON response includes a status, a message, and some data about a user."#;
        let parts: Vec<Vec<u8>> = llm_response
            .as_bytes()
            .chunks(30)
            .map(|chunk| chunk.to_vec())
            .collect();
        let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);
        tokio::spawn(async move {
            for part in parts {
                tx.send(part).await.unwrap();
                sleep(Duration::from_millis(10)).await;
            }
        });
        let response: LLMResponse = parser.next().await.unwrap();
        assert_eq!(response.status, "success");
        assert_eq!(response.data.name, "John Doe");
    }

    #[tokio::test]
    async fn test_async_json_parser_with_extra_text() {
        let text = r#"Some header info...
Here is the JSON:
{
  "name": "Alice",
  "age": 30
}
And here is some footer text."#;
        let parts: Vec<Vec<u8>> = text
            .as_bytes()
            .chunks(40)
            .map(|chunk| chunk.to_vec())
            .collect();
        let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);
        tokio::spawn(async move {
            for part in parts {
                tx.send(part).await.unwrap();
                sleep(Duration::from_millis(20)).await;
            }
        });
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }
}
