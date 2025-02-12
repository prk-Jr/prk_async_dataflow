#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use tokio::io::BufReader;

    use crate::AsyncJsonParser;

    #[tokio::test]
    async fn test_zero_copy_parsing() {
        let json_data = r#"{"key": "value"}"#;
        let stream = BufReader::new(Cursor::new(json_data.as_bytes()));
        let mut parser = AsyncJsonParser::new(stream);

        let value: serde_json::Value = parser.next().await.unwrap();
        assert_eq!(value, serde_json::json!({"key": "value"}));
    }

    #[tokio::test]
    async fn test_zero_copy_ndjson() {
        let ndjson = "{\"key\": \"value1\"}\n{\"key\": \"value2\"}"; // Use actual newline
        let stream = BufReader::new(Cursor::new(ndjson.as_bytes()));
        let mut parser = AsyncJsonParser::new(stream);

        let value1: serde_json::Value = parser.next_ndjson().await.unwrap();
        let value2: serde_json::Value = parser.next_ndjson().await.unwrap();
        assert_eq!(value1, serde_json::json!({"key": "value1"}));
        assert_eq!(value2, serde_json::json!({"key": "value2"}));
    }

}
