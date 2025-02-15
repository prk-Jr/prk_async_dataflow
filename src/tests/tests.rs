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
    struct Data {
        id: u32,
        name: String,
        roles: Vec<String>,
        enum_check: EnumCheck,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    enum EnumCheck {
        Value1,
        Value2,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct LLMResponse {
        status: String,
        message: String,
        data: Data,
        timestamp: String,
    }

    // --- Tests for extract_json_with_offset ---

    #[test]
    fn test_extract_json_with_offset_object() {
        let text = "Header { \"key\": \"value\" } Footer";
        let (json, consumed) = extract_json(text.as_bytes()).unwrap();
        assert_eq!(json, "{ \"key\": \"value\" }".as_bytes());
        assert!(consumed > 0);
    }

    #[test]
    fn test_extract_json_with_offset_array() {
        let text = "Some text [1, 2, 3, 4] more text";
        let (json, consumed) = extract_json(text.as_bytes()).unwrap();
        assert_eq!(json, "[1, 2, 3, 4]".as_bytes());
        assert!(consumed > 0);
    }

    // --- Tests for AsyncJsonParser with pure JSON ---

    #[tokio::test]
    async fn test_async_json_parser_single_chunk() {
        let data = b"{ \"name\": \"Alice\", \"age\": 30 }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }

    #[tokio::test]
    async fn test_async_json_parser_multiple_chunks() {
        let chunks: Vec<&[u8]> = vec![b"{ \"name\": \"A", b"lice\", \"age\":", b" 30 }"];
        let stream = BufReader::new(Cursor::new(chunks.into_iter().flat_map(|s| s.to_vec()).collect::<Vec<u8>>()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }

    #[tokio::test]
    #[should_panic(expected = "IncompleteData")]
    async fn test_async_json_parser_incomplete_data() {
        let data = b"{ \"name\": \"Alice\", \"age\": ";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        result.unwrap();
    }

    #[tokio::test]
    async fn test_async_json_parser_invalid_data() {
        let data = b"{ \"name\": \"Alice\", \"age\": invalid }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_async_json_parser_delayed_stream() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);
        tokio::spawn(async move {
            tx.send(b"{ \"name\": \"A".to_vec()).await.unwrap();
            sleep(Duration::from_millis(100)).await;
            tx.send(b"lice\", \"age\": 30 }".to_vec()).await.unwrap();
        });
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
    }

    #[tokio::test]
    async fn test_multiple_json_objects_in_stream() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);
        tokio::spawn(async move {
            tx.send(b"{\"name\":\"Alice\", \"age\": 30}".to_vec()).await.unwrap();
            sleep(Duration::from_millis(50)).await;
            tx.send(b"{\"name\":\"Bob\", \"age\": 25}".to_vec()).await.unwrap();
        });
        let person1: Person = parser.next().await.unwrap();
        assert_eq!(person1, Person { name: "Alice".into(), age: 30 });
        let person2: Person = parser.next().await.unwrap();
        assert_eq!(person2, Person { name: "Bob".into(), age: 25 });
    }

    #[tokio::test]
    async fn test_empty_channel() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(1);
        drop(tx);
        let stream = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_large_json_object_spanning_chunks() {
        let expected: Vec<i32> = (0..100).collect();
        let json_str = simd_json::to_string(&expected).unwrap();
        let mid = json_str.len() / 2;
        let part1 = json_str[..mid].as_bytes().to_vec();
        let part2 = json_str[mid..].as_bytes().to_vec();
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);
        tokio::spawn(async move {
            tx.send(part1).await.unwrap();
            sleep(Duration::from_millis(50)).await;
            tx.send(part2).await.unwrap();
        });
        let result: Vec<i32> = parser.next().await.unwrap();
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_json_with_whitespace() {
        let data = b"   \n\t { \"name\": \"Alice\", \"age\": 30 }  \n ";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(person, Person { name: "Alice".into(), age: 30 });
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
    "name": "John Doe",
    "email": "johndoe@example.com",
    "age": 30,
    "isActive": true,
    "enum_check": "value2",
    "roles": ["user", "admin"],
    "preferences": {
      "theme": "dark",
      "notifications": true
    }
  },
  "timestamp": "2023-10-05T12:34:56Z"
}
This JSON response includes a status, a message, some data about a user, and a timestamp. Let me know if you need a more specific or customized JSON response!"#;
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
        dbg!(&response);
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