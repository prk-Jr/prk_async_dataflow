#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simd_json::{json, OwnedValue};
    use tokio::io::BufReader;

    use crate::AsyncJsonParser;

    #[tokio::test]
    async fn test_zero_copy_parsing() {
        let json_data = r#"{"key": "value"}"#;
        let stream = BufReader::new(Cursor::new(json_data.as_bytes()));
        let mut parser = AsyncJsonParser::new(stream);

        let value: OwnedValue = parser.next().await.unwrap();
        assert_eq!(value, OwnedValue::from(json!({"key": "value"})));
    }

    #[tokio::test]
    async fn test_zero_copy_ndjson() {
        let ndjson = "{\"key\": \"value1\"}\n{\"key\": \"value2\"}"; // Use actual newline
        let stream = BufReader::new(Cursor::new(ndjson.as_bytes()));
        let mut parser = AsyncJsonParser::new(stream);

        // Parse the first batch (should contain both JSON objects)
        let batch: Vec<OwnedValue> = parser.next_batch().await.unwrap();

        // Verify the batch contains the expected objects
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0], OwnedValue::from(json!({"key": "value1"})));
        assert_eq!(batch[1], OwnedValue::from(json!({"key": "value2"})));
    }
}
