#[cfg(test)]
mod tests {
    use crate::{extract_json, AsyncJsonParser, ChannelReader};
    use std::io::Cursor;
    use tokio::io::BufReader;
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration};

    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq)]
    struct Person {
        name: String,
        age: u32,
    }

    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq)]
    struct LogEntry {
        level: String,
        message: String,
    }

    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq)]
    struct Data {
        id: u32,
        name: String,
    }

    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq)]
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
        let (json, consumed) = extract_json(text.as_bytes()).unwrap();
        assert_eq!(json, "{ \"key\": \"value\" }".as_bytes());
        assert!(consumed > 0);
    }

    #[test]
    fn test_extract_json_array() {
        let text = "Some text [1, 2, 3, 4] more text";
        let (json, consumed) = extract_json(text.as_bytes()).unwrap();
        assert_eq!(json, "[1, 2, 3, 4]".as_bytes());
        assert!(consumed > 0);
    }

    // --- Tests for AsyncJsonParser (strict mode) ---
    #[tokio::test]
    async fn test_async_json_parser_single_chunk() {
        let data = b"{ \"name\": \"Alice\", \"age\": 30 }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(
            person,
            Person {
                name: "Alice".into(),
                age: 30
            }
        );
    }

    #[tokio::test]
    async fn test_multiple_json_blocks_in_stream() {
        // Simulate two JSON objects concatenated in one stream with extra text in between.
        let data = r#"{"name": "Alice", "age": 30} Some extra text {"name": "Bob", "age": 25}"#;
        let stream = BufReader::new(Cursor::new(data.as_bytes().to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person1: Person = parser.next().await.unwrap();
        assert_eq!(
            person1,
            Person {
                name: "Alice".into(),
                age: 30
            }
        );
        let person2: Person = parser.next().await.unwrap();
        assert_eq!(
            person2,
            Person {
                name: "Bob".into(),
                age: 25
            }
        );
    }

    // --- Tests for NDJSON ---
    #[tokio::test]
    async fn test_next_ndjson_single_line() {
        let ndjson = r#"{"level": "INFO", "message": "Hello NDJSON"}"#;
        let stream = BufReader::new(Cursor::new(ndjson.as_bytes().to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let entry: LogEntry = parser.next().await.unwrap();
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
        let entry1: LogEntry = parser.next().await.unwrap();
        let entry2: LogEntry = parser.next().await.unwrap();
        let entry3: LogEntry = parser.next().await.unwrap();
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
    }

    // --- Tests for Robust Handling of Encoding Issues ---
    #[tokio::test]
    async fn test_encoding_issue_handling() {
        // Introduce some invalid UTF-8 bytes.
        let mut data = b"{ \"name\": \"Alice\", \"age\": 30 }".to_vec();
        // Append an invalid byte (e.g. 0xFF).
        data.push(0xFF);
        let stream = BufReader::new(Cursor::new(data));
        let mut parser = AsyncJsonParser::new(stream);
        // The parser uses from_utf8_lossy so it should still parse the valid JSON.
        let person: Person = parser.next().await.unwrap();
        assert_eq!(
            person,
            Person {
                name: "Alice".into(),
                age: 30
            }
        );
    }

    // --- Tests for Relaxed JSON Formats ---
    // To test this, enable the "relaxed" feature in Cargo.toml and add a dependency on json5.
    // For example, given a JSON5 input with trailing commas:
    #[cfg(feature = "relaxed")]
    #[tokio::test]
    async fn test_relaxed_json_parsing() {
        let data = b"{ \"name\": \"Alice\", \"age\": 30, }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(
            person,
            Person {
                name: "Alice".into(),
                age: 30
            }
        );
    }

    #[cfg(feature = "relaxed")]
    #[tokio::test]
    async fn test_relaxed_json_parsing_error_json() {
        let data = b"Here is your response:{name:'Prakash', age:30, extra:'remove me', yap:'   noisy message   '}";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(
            person,
            Person {
                name: "Prakash".into(),
                age: 30
            }
        );
    }

    // --- Tests for LLM response extraction (extra text around JSON) ---
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
Here is your data:
    {
      "name": "60c7cfb8-b2f1-4e80-b1f1-f8577d52ac7d",
      "age": 24
    },
    {
      "name": "6dc895e3-8d8a-4214-8e9d-8f331d7f0d7b",
      "age": 37
    },
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
        assert_eq!(
            person,
            Person {
                name: "60c7cfb8-b2f1-4e80-b1f1-f8577d52ac7d".into(),
                age: 24
            }
        );
    }
}